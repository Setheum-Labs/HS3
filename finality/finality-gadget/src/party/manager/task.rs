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

use std::{boxed::Box, pin::Pin};

use futures::channel::oneshot;
use log::warn;

use crate::Future;

/// A single handle that can be waited on, as returned by spawning an essential task.
pub type Handle = Pin<Box<(dyn Future<Output = sc_service::Result<(), ()>> + Send + 'static)>>;

/// A task that can be stopped or awaited until it stops itself.
pub struct Task {
    handle: Handle,
    exit: oneshot::Sender<()>,
    cached_result: Option<Result<(), ()>>,
}

impl Task {
    /// Create a new task.
    pub fn new(handle: Handle, exit: oneshot::Sender<()>) -> Self {
        Task {
            handle,
            exit,
            cached_result: None,
        }
    }

    /// Cleanly stop the task.
    pub async fn stop(self) -> Result<(), ()> {
        if let Some(result) = self.cached_result {
            return result;
        }
        if self.exit.send(()).is_err() {
            warn!(target: "stance-party", "Failed to send exit signal to authority");
        }
        self.handle.await
    }

    /// Await the task to stop by itself. Should usually just block forever, unless something went
    /// wrong. Can be called multiple times.
    pub async fn stopped(&mut self) -> Result<(), ()> {
        if let Some(result) = self.cached_result {
            return result;
        }
        let result = (&mut self.handle).await;
        self.cached_result = Some(result);
        result
    }
}
