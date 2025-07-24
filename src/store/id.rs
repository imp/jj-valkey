// Copyright 2025 Cyril Plisko
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Id(#[serde(with = "serde_bytes")] Vec<u8>);

impl Id {
    #[inline]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    fn _as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl std::ops::Deref for Id {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<&T> for Id
where
    T: jj::ObjectId,
{
    fn from(value: &T) -> Self {
        Self(value.to_bytes())
    }
}

impl From<Id> for jj::CommitId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::ChangeId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::FileId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::TreeId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::CopyId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::SymlinkId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<Id> for jj::ConflictId {
    fn from(value: Id) -> Self {
        Self::new(value.into_bytes())
    }
}
