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
pub struct Tree {
    entries: Vec<TreeEntry>,
}

impl Tree {
    pub fn from_backend(tree: &jj::Tree) -> Self {
        let entries = tree.entries().map(TreeEntry::from).collect();
        Self { entries }
    }
}

impl TryFrom<Tree> for jj::Tree {
    type Error = jj::InvalidNewRepoPathError;

    fn try_from(value: Tree) -> Result<Self, Self::Error> {
        value
            .entries
            .into_iter()
            .map(TreeEntry::try_into_parts)
            .collect::<Result<_, _>>()
            .map(Self::from_sorted_entries)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TreeEntry {
    name: String,
    value: TreeValue,
}

impl TreeEntry {
    fn try_into_parts(
        self,
    ) -> Result<(jj::RepoPathComponentBuf, jj::TreeValue), jj::InvalidNewRepoPathError> {
        let Self { name, value } = self;
        jj::RepoPathComponentBuf::new(name).map(|name| (name, value.into()))
    }
}

impl From<jj::TreeEntry<'_>> for TreeEntry {
    fn from(entry: jj::TreeEntry) -> Self {
        let name = entry.name().as_internal_str().to_string();
        let value = TreeValue::from_backend(entry.value());
        Self { name, value }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TreeValue {
    File {
        id: Id,
        executable: bool,
        copy_id: Id,
    },
    Symlink(Id),
    Tree(Id),
    GitSubmodule(Id),
    Conflict(Id),
}

impl TreeValue {
    pub fn from_backend(value: &jj::TreeValue) -> Self {
        match value {
            jj::TreeValue::File {
                id,
                executable,
                copy_id,
            } => Self::File {
                id: id.into(),
                executable: *executable,
                copy_id: copy_id.into(),
            },
            jj::TreeValue::Symlink(id) => Self::Symlink(id.into()),
            jj::TreeValue::Tree(id) => Self::Tree(id.into()),
            jj::TreeValue::GitSubmodule(id) => Self::GitSubmodule(id.into()),
            jj::TreeValue::Conflict(id) => Self::Conflict(id.into()),
        }
    }
}

impl From<TreeValue> for jj::TreeValue {
    fn from(value: TreeValue) -> Self {
        match value {
            TreeValue::File {
                id,
                executable,
                copy_id,
            } => Self::File {
                id: id.into(),
                executable,
                copy_id: copy_id.into(),
            },
            TreeValue::Symlink(id) => Self::Symlink(id.into()),
            TreeValue::Tree(id) => Self::Tree(id.into()),
            TreeValue::GitSubmodule(id) => Self::GitSubmodule(id.into()),
            TreeValue::Conflict(id) => Self::Conflict(id.into()),
        }
    }
}
