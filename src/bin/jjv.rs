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

use jj_valkey::ValkeyBackend;
use jj_valkey::jj;

#[derive(clap::Subcommand, Clone, Debug)]
enum Valkey {
    #[command(subcommand)]
    Valkey(ValkeyCommand),
}

/// Commands for working with Valkey remotes
#[derive(clap::Subcommand, Clone, Debug)]
enum ValkeyCommand {
    /// Initialize a workspace using the Valkey backend
    Init(ValkeyInitArgs),
}

fn run_custom_command(
    _ui: &mut jj::Ui,
    command_helper: &jj::CommandHelper,
    Valkey::Valkey(command): Valkey,
) -> Result<(), jj::CommandError> {
    match command {
        ValkeyCommand::Init(args) => {
            let cwd = command_helper.cwd();
            let workspace_root = cwd.join(&args.destination);
            let workspace_root = jj::file_util::create_or_reuse_dir(&workspace_root)
                .and_then(|()| dunce::canonicalize(workspace_root))
                .map_err(|err| jj::user_error_with_message("Failed to create workspace", err))?;
            let settings = command_helper.settings_for_new_workspace(&workspace_root)?;
            let url = args.url();
            // Initialize a workspace with the custom backend
            jj::Workspace::init_with_backend(
                &settings,
                &workspace_root,
                &|settings, store_path| {
                    Ok(Box::new(ValkeyBackend::init(&url, settings, store_path)?))
                },
                jj::Signer::from_settings(&settings).map_err(jj::WorkspaceInitError::SignInit)?,
            )?;
            Ok(())
        }
    }
}

fn main() -> std::process::ExitCode {
    jj::CliRunner::init()
        .add_store_factories(ValkeyBackend::store_factories())
        .add_subcommand(run_custom_command)
        .run()
        .into()
}

#[derive(clap::Args, Clone, Debug)]
pub struct ValkeyInitArgs {
    /// Valkey server host
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Valkey server port
    #[arg(long, default_value_t = 6379)]
    port: u16,

    /// Valkey authentication user
    #[arg(long)]
    user: Option<String>,

    // Valkey authentication password
    #[arg(long)]
    pass: Option<String>,

    /// The destination directory where the `jj` repo will be created.
    /// If the directory does not exist, it will be created.
    /// If no directory is given, the current directory is used.
    #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
    destination: String,
}

impl ValkeyInitArgs {
    fn url(&self) -> String {
        let host = &self.host;
        let port = self.port;
        match (&self.user, &self.pass) {
            (None, None) => format!("valkey://{host}:{port}/"),
            (None, Some(pass)) => format!("valkey://{pass}@{host}:{port}/"),
            (Some(_user), None) => todo!(),
            (Some(user), Some(pass)) => format!("valkey://{user}:{pass}@{host}:{port}/"),
        }
    }
}
