// بِسْمِ اللَّهِ الرَّحْمَنِ الرَّحِيم

// This file is part of DAGESTAN.

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

use std::{default::Default, sync::Arc};

use futures::channel::mpsc;
use log::{debug, error, warn};
use sc_client_api::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, NumberFor, One, Zero};

use crate::{
    data_io::{
        chain_info::{AuxFinalizationChainInfoProvider, CachedChainInfoProvider},
        status_provider::get_proposal_status,
        StanceData, ChainInfoProvider,
    },
    BlockHashNum, SessionBoundaries,
};

type InterpretersChainInfoProvider<B, C> =
    CachedChainInfoProvider<B, AuxFinalizationChainInfoProvider<B, Arc<C>>>;

/// Takes as input ordered `StanceData` from `Stance` and pushes blocks that should be finalized
/// to an output channel. The other end of the channel is held by the aggregator whose goal is to
/// create multisignatures under the finalized blocks.
pub struct OrderedDataInterpreter<B: BlockT, C: HeaderBackend<B>> {
    blocks_to_finalize_tx: mpsc::UnboundedSender<BlockHashNum<B>>,
    chain_info_provider: InterpretersChainInfoProvider<B, C>,
    last_finalized_by_stance: BlockHashNum<B>,
    session_boundaries: SessionBoundaries<B>,
}

fn get_last_block_prev_session<B: BlockT, C: HeaderBackend<B>>(
    session_boundaries: SessionBoundaries<B>,
    mut client: Arc<C>,
) -> BlockHashNum<B> {
    if session_boundaries.first_block() > NumberFor::<B>::zero() {
        // We are in session > 0, we take the last block of previous session.
        let last_prev_session_num = session_boundaries.first_block() - NumberFor::<B>::one();
        client.get_finalized_at(last_prev_session_num).expect(
            "Last block of previous session must have been finalized before starting the current",
        )
    } else {
        // We are in session 0, we take the genesis block -- it is finalized by definition.
        client
            .get_finalized_at(NumberFor::<B>::zero())
            .expect("Genesis block must be available")
    }
}

impl<B: BlockT, C: HeaderBackend<B>> OrderedDataInterpreter<B, C> {
    pub fn new(
        blocks_to_finalize_tx: mpsc::UnboundedSender<BlockHashNum<B>>,
        client: Arc<C>,
        session_boundaries: SessionBoundaries<B>,
    ) -> Self {
        let last_finalized_by_stance =
            get_last_block_prev_session(session_boundaries.clone(), client.clone());
        let chain_info_provider =
            AuxFinalizationChainInfoProvider::new(client, last_finalized_by_stance.clone());
        let chain_info_provider =
            CachedChainInfoProvider::new(chain_info_provider, Default::default());

        OrderedDataInterpreter {
            blocks_to_finalize_tx,
            chain_info_provider,
            last_finalized_by_stance,
            session_boundaries,
        }
    }

    fn blocks_to_finalize_from_data(&mut self, new_data: StanceData<B>) -> Vec<BlockHashNum<B>> {
        let unvalidated_proposal = new_data.head_proposal;
        let proposal = match unvalidated_proposal.validate_bounds(&self.session_boundaries) {
            Ok(proposal) => proposal,
            Err(error) => {
                warn!(target: "stance-finality", "Incorrect proposal {:?} passed through data availability, session bounds: {:?}, error: {:?}", unvalidated_proposal, self.session_boundaries, error);
                return Vec::new();
            }
        };

        // WARNING: If we ever enable block pruning, this code (and the code in Data Store) must be carefully
        // analyzed for possible safety violations.

        use crate::data_io::proposal::ProposalStatus::*;
        let status = get_proposal_status(&mut self.chain_info_provider, &proposal, None);
        match status {
            Finalize(blocks) => blocks,
            Ignore => {
                debug!(target: "stance-finality", "Ignoring proposal {:?} in interpreter.", proposal);
                Vec::new()
            }
            Pending(pending_status) => {
                panic!(
                    "Pending proposal {:?} with status {:?} encountered in Data.",
                    proposal, pending_status
                );
            }
        }
    }
}

impl<B: BlockT, C: HeaderBackend<B> + Send + 'static> stance::FinalizationHandler<StanceData<B>>
    for OrderedDataInterpreter<B, C>
{
    fn data_finalized(&mut self, data: StanceData<B>) {
        for block in self.blocks_to_finalize_from_data(data) {
            self.last_finalized_by_stance = block.clone();
            self.chain_info_provider
                .inner()
                .update_aux_finalized(block.clone());
            if let Err(err) = self.blocks_to_finalize_tx.unbounded_send(block) {
                error!(target: "stance-finality", "Error in sending a block from FinalizationHandler, {}", err);
            }
        }
    }
}
