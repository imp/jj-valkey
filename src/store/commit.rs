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
pub struct Commit {
    parents: Vec<Id>,
    predecessors: Vec<Id>,
    root_tree: MergedTreeId,
    change_id: Id,
    description: String,
    author: Signature,
    committer: Signature,
    secure_sig: Option<SecureSig>,
}

impl Commit {
    pub fn from_backend(commit: &jj::Commit) -> Self {
        Self::from(commit)
    }

    pub fn signature(&mut self, sig: &[u8]) {
        self.secure_sig = Some(SecureSig(sig.to_vec()));
    }

    pub fn take_signature(&mut self) -> Option<Vec<u8>> {
        self.secure_sig.take().map(|SecureSig(buf)| buf)
    }
}

impl From<&jj::Commit> for Commit {
    fn from(value: &jj::Commit) -> Self {
        let jj::Commit {
            parents,
            predecessors,
            root_tree,
            change_id,
            description,
            author,
            committer,
            secure_sig,
        } = value;

        let parents = parents.iter().map(Into::into).collect();
        let predecessors = predecessors.iter().map(Into::into).collect();
        let root_tree = root_tree.into();
        let change_id = change_id.into();
        let description = description.clone();
        let author = author.into();
        let committer = committer.into();
        assert!(secure_sig.is_none());
        let secure_sig = None;

        Self {
            parents,
            predecessors,
            root_tree,
            change_id,
            description,
            author,
            committer,
            secure_sig,
        }
    }
}

impl From<Commit> for jj::Commit {
    fn from(value: Commit) -> Self {
        let Commit {
            parents,
            predecessors,
            root_tree,
            change_id,
            description,
            author,
            committer,
            secure_sig,
        } = value;

        let parents = parents.into_iter().map(Into::into).collect();
        let predecessors = predecessors.into_iter().map(Into::into).collect();
        let root_tree = root_tree.into();
        let change_id = change_id.into();
        let author = author.into();
        let committer = committer.into();
        assert!(secure_sig.is_none());
        let secure_sig = None;

        Self {
            parents,
            predecessors,
            root_tree,
            change_id,
            description,
            author,
            committer,
            secure_sig,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Signature {
    name: String,
    email: String,
    timestamp: Timestamp,
}

impl From<&jj::Signature> for Signature {
    fn from(value: &jj::Signature) -> Self {
        let jj::Signature {
            name,
            email,
            timestamp,
        } = value;
        let name = name.clone();
        let email = email.clone();
        let timestamp = timestamp.into();
        Self {
            name,
            email,
            timestamp,
        }
    }
}

impl From<Signature> for jj::Signature {
    fn from(value: Signature) -> Self {
        let Signature {
            name,
            email,
            timestamp,
        } = value;
        let timestamp = timestamp.into();
        Self {
            name,
            email,
            timestamp,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Timestamp {
    millis: i64,
    tz_offset: i32,
}

impl From<&jj::Timestamp> for Timestamp {
    fn from(value: &jj::Timestamp) -> Self {
        Self {
            millis: value.timestamp.0,
            tz_offset: value.tz_offset,
        }
    }
}

impl From<Timestamp> for jj::Timestamp {
    fn from(value: Timestamp) -> Self {
        Self {
            timestamp: jj::MillisSinceEpoch(value.millis),
            tz_offset: value.tz_offset,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct MergedTreeId {
    legacy: bool,
    values: Vec<Id>,
}

impl From<&jj::MergedTreeId> for MergedTreeId {
    fn from(value: &jj::MergedTreeId) -> Self {
        match value {
            jj::MergedTreeId::Legacy(tree_id) => Self {
                legacy: true,
                values: vec![tree_id.into()],
            },
            jj::MergedTreeId::Merge(merge) => Self {
                legacy: false,
                values: merge
                    .iter()
                    .map(jj::ObjectId::to_bytes)
                    .map(Id::new)
                    .collect(),
            },
        }
    }
}

impl From<MergedTreeId> for jj::MergedTreeId {
    fn from(value: MergedTreeId) -> Self {
        let MergedTreeId { legacy, mut values } = value;
        if legacy {
            let Some(id) = values.pop() else {
                panic!("Non single Legacy tree id");
            };
            let id = id.into();
            Self::Legacy(id)
        } else {
            let values = values.into_iter().map(jj::TreeId::from).collect::<Vec<_>>();
            let merge = jj_lib::merge::Merge::from_vec(values);
            Self::Merge(merge)
        }
    }
}
