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
pub struct Conflict {
    removes: Vec<ConflictTerm>,
    adds: Vec<ConflictTerm>,
}

impl Conflict {
    pub fn from_backend(conflict: &jj::Conflict) -> Self {
        let jj::Conflict { removes, adds } = conflict;
        let removes = removes.iter().map(ConflictTerm::from_backend).collect();
        let adds = adds.iter().map(ConflictTerm::from_backend).collect();
        Self { removes, adds }
    }
}

impl From<Conflict> for jj::Conflict {
    fn from(conflict: Conflict) -> Self {
        let Conflict { removes, adds } = conflict;
        let removes = removes.into_iter().map(Into::into).collect();
        let adds = adds.into_iter().map(Into::into).collect();
        Self { removes, adds }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConflictTerm {
    value: TreeValue,
}

impl ConflictTerm {
    fn from_backend(term: &jj::ConflictTerm) -> Self {
        let value = TreeValue::from_backend(&term.value);
        Self { value }
    }
}

impl From<ConflictTerm> for jj::ConflictTerm {
    fn from(term: ConflictTerm) -> Self {
        let value = term.value.into();
        Self { value }
    }
}
