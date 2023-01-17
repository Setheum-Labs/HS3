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

use std::{fmt::Debug, hash::Hash};

use stance_rmc::Message;
use stance_types::Recipient;

mod aggregator;
mod multicast;

pub use crate::{
    aggregator::{BlockSignatureAggregator, IO},
    multicast::SignableHash,
};

pub type RmcNetworkData<H, S, SS> = Message<SignableHash<H>, S, SS>;

#[derive(Debug)]
pub enum NetworkError {
    SendFail,
}

#[async_trait::async_trait]
pub trait ProtocolSink<D>: Send + Sync {
    async fn next(&mut self) -> Option<D>;
    fn send(&self, data: D, recipient: Recipient) -> Result<(), NetworkError>;
}

pub trait Metrics<H: Debug + Hash + Eq + Debug + Copy> {
    fn report_aggregation_complete(&mut self, h: H);
}
