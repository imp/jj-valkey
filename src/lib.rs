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

use std::any::Any;
use std::error::Error as StdError;
use std::fs;
use std::path::Path;
use std::pin::Pin;
use std::time::SystemTime;

use futures::stream;
use futures::stream::BoxStream;
use pollster::FutureExt;
use redis::Commands as _;
use redis::IntoConnectionInfo as _;
use redis::RedisResult;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json as json;
use tokio::io;
use tokio::io::AsyncReadExt;
use toml_edit as toml;

use jj::Backend;
use jj::ObjectId;

mod store;

pub mod jj;

const COMMIT_ID_LENGTH: usize = 64;
const CHANGE_ID_LENGTH: usize = 16;

#[derive(Debug)]
pub struct ValkeyBackend {
    config: ValkeyRepoConfig,
    client: redis::Client,
    commit_key: String,
    file_key: String,
    symlink_key: String,
    tree_key: String,
    conflict_key: String,
    root_commit_id: jj::CommitId,
    root_change_id: jj::ChangeId,
    empty_tree_id: jj::TreeId,
}

impl ValkeyBackend {
    const CONFIG_FILE: &str = "valkey_config.toml";
    const NAME: &str = "valkey";

    pub fn store_factories() -> jj::StoreFactories {
        let mut store_factories = jj::StoreFactories::empty();
        // Register the backend so it can be loaded when the repo is loaded. The name
        // must match `Backend::name()`.
        store_factories.add_backend(
            Self::NAME,
            Box::new(|settings, store_path| Ok(Box::new(Self::load(settings, store_path)?))),
        );
        store_factories
    }

    pub fn init(
        url: impl ToString,
        settings: &jj::UserSettings,
        store_path: &Path,
    ) -> Result<Self, jj::BackendInitError> {
        println!("User: {}", settings.user_name());
        println!("Commit timestamp: {:?}", settings.commit_timestamp());
        println!("Store path: {}", store_path.display());
        let prefix = settings.commit_timestamp().map_or_else(
            || "somerandomstring".to_string(),
            |ts| ts.timestamp.0.to_string(),
        );
        let config = ValkeyRepoConfig::new(prefix, url);
        let config_file = store_path.join(Self::CONFIG_FILE);
        config
            .save_to_file(config_file)
            .map_err(backend_init_error)?;

        let backend = Self::load_from_store_path(store_path).map_err(backend_init_error)?;
        let empty_tree_id = backend
            .write_tree(jj::RepoPath::root(), &jj::Tree::default())
            .block_on()
            .map_err(backend_init_error)?;
        assert_eq!(empty_tree_id, backend.empty_tree_id);

        Ok(backend)
    }

    pub fn load(
        _settings: &jj::UserSettings,
        store_path: &Path,
    ) -> Result<Self, jj::BackendLoadError> {
        Self::load_from_store_path(store_path).map_err(backend_load_error)
    }

    fn load_from_store_path(store_path: &Path) -> RedisResult<Self> {
        let config_file = store_path.join(Self::CONFIG_FILE);
        let config = ValkeyRepoConfig::load_from_file(config_file)?;
        Self::from_config(config).inspect(|backend| println!("{backend:?}"))
    }

    fn from_config(config: ValkeyRepoConfig) -> RedisResult<Self> {
        let client = config.connection_info().and_then(redis::Client::open)?;
        let prefix = &config.prefix;
        let file_key = format!("{prefix}:files");
        let commit_key = format!("{prefix}:commits");
        let symlink_key = format!("{prefix}:symlinks");
        let tree_key = format!("{prefix}:trees");
        let conflict_key = format!("{prefix}:conflicts");
        let root_commit_id = jj::CommitId::from_bytes(&[0; COMMIT_ID_LENGTH]);
        let root_change_id = jj::ChangeId::from_bytes(&[0; CHANGE_ID_LENGTH]);
        let empty_tree_id = jj::TreeId::from_hex(
            "482ae5a29fbe856c7272f2071b8b0f0359ee2d89ff392b8a900643fbd0836eccd067b8bf41909e206c90d45d6e7d8b6686b93ecaee5fe1a9060d87b672101310",
        );

        Ok(Self {
            config,
            client,
            commit_key,
            file_key,
            symlink_key,
            tree_key,
            conflict_key,
            root_commit_id,
            root_change_id,
            empty_tree_id,
        })
    }

    fn connection(&self) -> RedisResult<redis::Connection> {
        self.client.get_connection()
    }

    fn put_object(
        &self,
        key: impl AsRef<str>,
        field: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> RedisResult<u64> {
        self.connection()?
            .hset(key.as_ref(), field.as_ref(), value.as_ref())
    }

    fn get_object(
        &self,
        key: impl AsRef<str>,
        field: impl AsRef<[u8]>,
    ) -> RedisResult<Option<Vec<u8>>> {
        self.connection()?.hget(key.as_ref(), field.as_ref())
    }

    fn get_file(&self, id: &jj::FileId) -> jj::BackendResult<Vec<u8>> {
        let field = id.as_bytes();
        self.get_object(&self.file_key, field)
            .map_err(|err| read_object_error(id, err))?
            .ok_or_else(|| object_not_found(id))
    }

    fn put_file(&self, id: &jj::FileId, bytes: &[u8]) -> jj::BackendResult<u64> {
        let field = id.as_bytes();
        self.put_object(&self.file_key, field, bytes)
            .map_err(|err| write_object_error("file", err))
    }

    fn get_conflict(&self, id: &jj::ConflictId) -> jj::BackendResult<store::Conflict> {
        let field = id.as_bytes();
        let buf = self
            .get_object(&self.conflict_key, field)
            .map_err(|err| read_object_error(id, err))?
            .ok_or_else(|| object_not_found(id))?;
        self.decode_from_vec(buf)
    }

    fn put_conflict(
        &self,
        id: &jj::ConflictId,
        conflict: &store::Conflict,
    ) -> jj::BackendResult<u64> {
        let field = id.as_bytes();
        let value = self.encode_to_vec(conflict)?;
        self.put_object(&self.conflict_key, field, value)
            .map_err(|err| write_object_error("conflict", err))
    }

    fn get_commit(&self, id: &jj::CommitId) -> jj::BackendResult<store::Commit> {
        let field = id.as_bytes();
        let buf = self
            .get_object(&self.commit_key, field)
            .map_err(|err| read_object_error(id, err))?
            .ok_or_else(|| object_not_found(id))?;
        self.decode_from_vec(buf)
    }

    fn put_commit(&self, id: &jj::CommitId, commit: &store::Commit) -> jj::BackendResult<u64> {
        let field = id.as_bytes();
        let value = self.encode_to_vec(commit)?;
        self.put_object(&self.commit_key, field, value)
            .map_err(|err| write_object_error("commit", err))
    }

    fn get_tree(&self, id: &jj::TreeId) -> jj::BackendResult<store::Tree> {
        let field = id.as_bytes();
        let buf = self
            .get_object(&self.tree_key, field)
            .map_err(|err| read_object_error(id, err))?
            .ok_or_else(|| object_not_found(id))?;
        self.decode_from_vec(buf)
    }

    fn put_tree(&self, id: &jj::TreeId, tree: &store::Tree) -> jj::BackendResult<u64> {
        let field = id.as_bytes();
        let value = self.encode_to_vec(tree)?;
        self.put_object(&self.tree_key, field, value)
            .map_err(|err| write_object_error("tree", err))
    }

    fn encode_to_vec<T>(&self, value: &T) -> jj::BackendResult<Vec<u8>>
    where
        T: Serialize,
    {
        match self.config.encoding {
            Encoding::Json => json::to_vec(value).map_err(to_other_err),
        }
    }

    fn decode_from_vec<T>(&self, buf: impl AsRef<[u8]>) -> jj::BackendResult<T>
    where
        T: DeserializeOwned,
    {
        let buf = buf.as_ref();
        match self.config.encoding {
            Encoding::Json => json::from_slice(buf).map_err(to_other_err),
        }
    }
}

#[async_trait::async_trait]
impl jj::Backend for ValkeyBackend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// A unique name that identifies this backend. Written to
    /// `.jj/repo/store/type` when the repo is created.
    fn name(&self) -> &str {
        Self::NAME
    }

    /// The length of commit IDs in bytes.
    fn commit_id_length(&self) -> usize {
        COMMIT_ID_LENGTH
    }

    /// The length of change IDs in bytes.
    fn change_id_length(&self) -> usize {
        CHANGE_ID_LENGTH
    }

    fn root_commit_id(&self) -> &jj::CommitId {
        &self.root_commit_id
    }

    fn root_change_id(&self) -> &jj::ChangeId {
        &self.root_change_id
    }

    fn empty_tree_id(&self) -> &jj::TreeId {
        &self.empty_tree_id
    }

    /// An estimate of how many concurrent requests this backend handles well. A
    /// local backend like the Git backend (at until it supports partial clones)
    /// may want to set this to 1. A cloud-backed backend may want to set it to
    /// 100 or so.
    ///
    /// It is not guaranteed that at most this number of concurrent requests are
    /// sent.
    fn concurrency(&self) -> usize {
        32
    }

    async fn read_file(
        &self,
        _path: &jj::RepoPath,
        id: &jj::FileId,
    ) -> jj::BackendResult<Pin<Box<dyn io::AsyncRead + Send>>> {
        let bytes = self.get_file(id)?;
        Ok(Box::pin(std::io::Cursor::new(bytes)))
    }

    async fn write_file(
        &self,
        _path: &jj::RepoPath,
        contents: &mut (dyn io::AsyncRead + Send + Unpin),
    ) -> jj::BackendResult<jj::FileId> {
        let mut bytes = Vec::with_capacity(1024);
        let _count = contents
            .read_to_end(&mut bytes)
            .await
            .map_err(to_other_err)?;
        let id = jj::FileId::new(jj::blake2b_hash(&bytes).to_vec());
        let count = self.put_file(&id, &bytes)?;
        debug_assert_eq!(count, 1);
        Ok(id)
    }

    async fn read_symlink(
        &self,
        path: &jj::RepoPath,
        id: &jj::SymlinkId,
    ) -> jj::BackendResult<String> {
        todo!("read symlink {path:?} {id} ({})", self.symlink_key)
    }

    async fn write_symlink(
        &self,
        path: &jj::RepoPath,
        target: &str,
    ) -> jj::BackendResult<jj::SymlinkId> {
        todo!("write symlink {path:?} {target}")
    }

    /// Read the specified `CopyHistory` object.
    ///
    /// Backends that don't support copy tracking may return
    /// `BackendError::Unsupported`.
    async fn read_copy(&self, id: &jj::CopyId) -> jj::BackendResult<jj::CopyHistory> {
        todo!("read copy {id}")
    }

    /// Write the `CopyHistory` object and return its ID.
    ///
    /// Backends that don't support copy tracking may return
    /// `BackendError::Unsupported`.
    async fn write_copy(&self, copy: &jj::CopyHistory) -> jj::BackendResult<jj::CopyId> {
        todo!("write copy {copy:?}")
    }

    /// Find all copy histories that are related to the specified one. This is
    /// defined as those that are ancestors of the given specified one, plus
    /// their descendants. Children must be returned before parents.
    ///
    /// It is valid (but wasteful) to include other copy histories, such as
    /// siblings, or even completely unrelated copy histories.
    ///
    /// Backends that don't support copy tracking may return
    /// `BackendError::Unsupported`.
    async fn get_related_copies(
        &self,
        copy_id: &jj::CopyId,
    ) -> jj::BackendResult<Vec<jj::CopyHistory>> {
        todo!("get related copies {copy_id}")
    }

    async fn read_tree(
        &self,
        _path: &jj::RepoPath,
        id: &jj::TreeId,
    ) -> jj::BackendResult<jj::Tree> {
        let stored_tree = self.get_tree(id)?;
        stored_tree.try_into().map_err(to_other_err)
    }

    async fn write_tree(
        &self,
        _path: &jj::RepoPath,
        tree: &jj::Tree,
    ) -> jj::BackendResult<jj::TreeId> {
        let id = jj::TreeId::new(jj::blake2b_hash(tree).to_vec());
        let stored_tree = store::Tree::from_backend(tree);
        let count = self.put_tree(&id, &stored_tree)?;
        debug_assert_eq!(count, 1, "One object should be written");
        Ok(id)
    }

    // Not async because it would force `MergedTree::value()` to be async. We don't
    // need this to be async anyway because it's only used by legacy repos.
    fn read_conflict(
        &self,
        _path: &jj::RepoPath,
        id: &jj::ConflictId,
    ) -> jj::BackendResult<jj::Conflict> {
        self.get_conflict(id).map(Into::into)
    }

    fn write_conflict(
        &self,
        _path: &jj::RepoPath,
        conflict: &jj::Conflict,
    ) -> jj::BackendResult<jj::ConflictId> {
        let id = jj::ConflictId::new(jj::blake2b_hash(conflict).to_vec());
        let stored_conflict = store::Conflict::from_backend(conflict);
        let count = self.put_conflict(&id, &stored_conflict)?;
        debug_assert_eq!(count, 1, "One object should be written");
        Ok(id)
    }

    async fn read_commit(&self, id: &jj::CommitId) -> jj::BackendResult<jj::Commit> {
        if *id == self.root_commit_id {
            return Ok(jj::make_root_commit(
                self.root_change_id().clone(),
                self.empty_tree_id.clone(),
            ));
        }

        let mut stored_commit = self.get_commit(id)?;
        let secure_sig = match stored_commit.take_signature() {
            Some(sig) => {
                let data = self.encode_to_vec(&stored_commit)?;
                Some(jj::SecureSig { data, sig })
            }
            None => None,
        };

        Ok(jj::Commit {
            secure_sig,
            ..jj::Commit::from(stored_commit)
        })
    }

    /// Writes a commit and returns its ID and the commit itself. The commit
    /// should contain the data that was actually written, which may differ
    /// from the data passed in. For example, the backend may change the
    /// committer name to an authenticated user's name, or the backend's
    /// timestamps may have less precision than the millisecond precision in
    /// `Commit`.
    ///
    /// The `sign_with` parameter could contain a function to cryptographically
    /// sign some binary representation of the commit.
    /// If the backend supports it, it could call it and store the result in
    /// an implementation specific fashion, and both `read_commit` and the
    /// return of `write_commit` should read it back as the `secure_sig`
    /// field.
    async fn write_commit(
        &self,
        mut commit: jj::Commit,
        sign_with: Option<&mut jj::SigningFn>,
    ) -> jj::BackendResult<(jj::CommitId, jj::Commit)> {
        assert!(commit.secure_sig.is_none(), "commit.secure_sig was set");

        if commit.parents.is_empty() {
            return Err(jj::BackendError::Other(
                "Cannot write a commit with no parents".into(),
            ));
        }

        let mut stored_commit = store::Commit::from_backend(&commit);

        if let Some(sign) = sign_with {
            let data = self.encode_to_vec(&stored_commit)?;
            let sig = sign(&data).map_err(to_other_err)?;
            stored_commit.signature(&sig);
            commit.secure_sig = Some(jj::SecureSig { data, sig });
        }

        let id = jj::CommitId::new(jj::blake2b_hash(&commit).to_vec());
        let count = self.put_commit(&id, &stored_commit)?;
        assert_eq!(count, 1, "Should store exactly one object");
        Ok((id, commit))
    }

    /// Get copy records for the dag range `root..head`.  If `paths` is None
    /// include all paths, otherwise restrict to only `paths`.
    ///
    /// The exact order these are returned is unspecified, but it is guaranteed
    /// to be reverse-topological. That is, for any two copy records with
    /// different commit ids A and B, if A is an ancestor of B, A is streamed
    /// after B.
    ///
    /// Streaming by design to better support large backends which may have very
    /// large single-file histories. This also allows more iterative algorithms
    /// like blame/annotate to short-circuit after a point without wasting
    /// unnecessary resources.
    fn get_copy_records(
        &self,
        _paths: Option<&[jj::RepoPathBuf]>,
        _root: &jj::CommitId,
        _head: &jj::CommitId,
    ) -> jj::BackendResult<BoxStream<'_, jj::BackendResult<jj::CopyRecord>>> {
        // todo!("get copy records {paths:?} {root} {head}");
        Ok(Box::pin(stream::empty()))
    }

    /// Perform garbage collection.
    ///
    /// All commits found in the `index` won't be removed. In addition to that,
    /// objects created after `keep_newer` will be preserved. This mitigates a
    /// risk of deleting new commits created concurrently by another process.
    fn gc(&self, _index: &dyn jj::Index, keep_newer: SystemTime) -> jj::BackendResult<()> {
        todo!("gc {keep_newer:?}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Encoding {
    Json,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ValkeyRepoConfig {
    prefix: String,
    url: String,
    encoding: Encoding,
}

impl ValkeyRepoConfig {
    fn new(prefix: impl ToString, url: impl ToString) -> Self {
        let prefix = prefix.to_string();
        let url = url.to_string();
        let encoding = Encoding::Json;
        Self {
            prefix,
            url,
            encoding,
        }
    }

    fn connection_info(&self) -> redis::RedisResult<redis::ConnectionInfo> {
        self.url.as_str().into_connection_info()
    }

    fn load_from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let slice = fs::read(path)?;
        toml::de::from_slice(&slice).map_err(io::Error::other)
    }

    fn save_to_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let slice = toml::ser::to_vec(self).map_err(io::Error::other)?;
        fs::write(path, slice)
    }
}

fn to_other_err(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> jj::BackendError {
    jj::BackendError::Other(err.into())
}

fn object_not_found(id: &impl jj::ObjectId) -> jj::BackendError {
    let object_type = id.object_type();
    let hash = id.hex();
    let source = Box::new(io::Error::new(io::ErrorKind::NotFound, "No such object"));
    jj::BackendError::ObjectNotFound {
        object_type,
        hash,
        source,
    }
}

fn backend_init_error(
    err: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> jj::BackendInitError {
    jj::BackendInitError(err.into())
}

fn backend_load_error(
    err: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> jj::BackendLoadError {
    jj::BackendLoadError(err.into())
}

fn read_object_error<T, E>(id: &T, err: E) -> jj::BackendError
where
    T: jj::ObjectId,
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    jj::BackendError::ReadObject {
        object_type: id.object_type(),
        hash: id.hex(),
        source: err.into(),
    }
}

fn write_object_error<E>(object_type: &'static str, err: E) -> jj::BackendError
where
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    jj::BackendError::WriteObject {
        object_type,
        source: err.into(),
    }
}
