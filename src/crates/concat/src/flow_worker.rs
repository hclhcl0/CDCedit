// SPDX-License-Identifier: AGPL-3.0-or-later
//! Desktop Flow worker shim delegating to the process-wide [`concat_host::flow::FlowAccountLock`].

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use concat_host::flow::FlowAccountLock;
use serde_json::Value;

pub(crate) fn rendering_accounts() -> HashSet<String> {
    FlowAccountLock::shared().rendering_accounts()
}

pub(crate) fn set_limit(limit: usize) {
    FlowAccountLock::shared().set_limit(limit);
}

pub(crate) fn in_use(dir: &Path) -> bool {
    FlowAccountLock::shared().in_use(dir)
}

pub(crate) fn call_account(
    root: &Path,
    account: &crate::flow_accounts::Account,
    tool: &str,
    args: Value,
    allow_credit: bool,
) -> Result<Value, String> {
    FlowAccountLock::shared().call_account(root, account, tool, args, allow_credit)
}

pub fn clean_path(path: PathBuf) -> PathBuf {
    concat_host::flow::clean_path(path)
}

pub fn result_files(state_dir: &Path, value: &Value) -> Vec<PathBuf> {
    concat_host::flow::result_files(state_dir, value)
}

pub(crate) fn generated_images(dir: &Path, value: &Value) -> Vec<PathBuf> {
    concat_host::flow::generated_images(dir, value)
}

pub fn imported_image(state_dir: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    concat_host::flow::imported_image(state_dir, value)
}
