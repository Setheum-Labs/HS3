// بِسْمِ اللَّهِ الرَّحْمَنِ الرَّحِيم

// This file is part of STANCE.

// Copyright (C) 2019-Present Setheum Labs.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use futures::{
    channel::{mpsc, oneshot},
    FutureExt,
};
use log::debug;

use crate::{
    config::Config,
    creation,
    extender::Extender,
    handle_task_termination,
    runway::{NotificationIn, NotificationOut},
    terminal::Terminal,
    Hasher, Receiver, Round, Sender, SpawnHandle, Terminator,
};

pub(crate) async fn run<H: Hasher + 'static>(
    conf: Config,
    incoming_notifications: Receiver<NotificationIn<H>>,
    outgoing_notifications: Sender<NotificationOut<H>>,
    ordered_batch_tx: Sender<Vec<H::Hash>>,
    spawn_handle: impl SpawnHandle,
    starting_round: oneshot::Receiver<Option<Round>>,
    mut terminator: Terminator,
) {
    debug!(target: "StanceBFT", "{:?} Starting all services...", conf.node_ix);

    let n_members = conf.n_members;
    let index = conf.node_ix;

    let (electors_tx, electors_rx) = mpsc::unbounded();
    let mut extender = Extender::<H>::new(index, n_members, electors_rx, ordered_batch_tx);
    let extender_terminator = terminator.add_offspring_connection("StanceBFT-extender");
    let mut extender_handle = spawn_handle
        .spawn_essential("consensus/extender", async move {
            extender.extend(extender_terminator).await
        })
        .fuse();

    let (parents_for_creator, parents_from_terminal) = mpsc::unbounded();

    let creator_terminator = terminator.add_offspring_connection("creator");
    let io = creation::IO {
        outgoing_units: outgoing_notifications.clone(),
        incoming_parents: parents_from_terminal,
    };
    let mut creator_handle = spawn_handle
        .spawn_essential("consensus/creation", async move {
            creation::run(conf.clone().into(), io, starting_round, creator_terminator).await;
        })
        .fuse();

    let mut terminal = Terminal::new(index, incoming_notifications, outgoing_notifications);

    // send a new parent candidate to the creator
    terminal.register_post_insert_hook(Box::new(move |u| {
        parents_for_creator
            .unbounded_send(u.into())
            .expect("Channel to creator should be open.");
    }));
    // try to extend the partial order after adding a unit to the dag
    terminal.register_post_insert_hook(Box::new(move |u| {
        electors_tx
            .unbounded_send(u.into())
            .expect("Channel to extender should be open.")
    }));

    let terminal_terminator = terminator.add_offspring_connection("terminal");
    let mut terminal_handle = spawn_handle
        .spawn_essential("consensus/terminal", async move {
            terminal.run(terminal_terminator).await
        })
        .fuse();
    debug!(target: "StanceBFT", "{:?} All services started.", index);

    futures::select! {
        _ = terminator.get_exit() => {},
        _ = terminal_handle => {
            debug!(target: "StanceBFT-consensus", "{:?} terminal task terminated early.", index);
        },
        _ = creator_handle => {
            debug!(target: "StanceBFT-consensus", "{:?} creator task terminated early.", index);
        },
        _ = extender_handle => {
            debug!(target: "StanceBFT-consensus", "{:?} extender task terminated early.", index);
        }
    }
    debug!(target: "StanceBFT", "{:?} All services stopping.", index);

    // we stop no matter if received Ok or Err
    terminator.terminate_sync().await;

    handle_task_termination(terminal_handle, "StanceBFT-consensus", "Terminal", index).await;
    handle_task_termination(creator_handle, "StanceBFT-consensus", "Creator", index).await;
    handle_task_termination(extender_handle, "StanceBFT-consensus", "Extender", index).await;

    debug!(target: "StanceBFT", "{:?} All services stopped.", index);
}
