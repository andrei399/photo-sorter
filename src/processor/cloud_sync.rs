//! Uploads, as a diff: every enabled provider should hold each favourite's files (`Session::favourites`); what it
//! does hold is `Session::remote_file_ids`. The difference is the job list for the cloud worker, so there is no
//! separate queue to persist or drift: offline for a week, restarted, un-favourited mid-upload, it all converges.

use crate::error::{AppError, AppResult};
use crate::integrations::cloud::{CloudJob, CloudPlan, CloudProblem, CloudReport, JobKey};
use crate::models::cloud::{CloudAccount, CloudProvider};
use crate::models::session::Session;
use crate::models::state::PersistedState;
use crate::models::store::Store;
use crate::processor::sessions::session_name_to_folder_name;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudStatus {
    pub provider: CloudProvider,
    /// `false` when this build has no app registration for the provider, so it can't be connected.
    pub can_connect: bool,
    pub account: Option<CloudAccount>,
    /// Uploads and deletions still to do.
    pub pending: usize,
    pub problem: Option<CloudProblem>,
}

pub fn build_cloud_plan(state: &PersistedState, reports_applied: u64) -> CloudPlan {
    let accounts: Vec<CloudAccount> = state
        .cloud_accounts
        .iter()
        .filter(|account| account.enabled)
        .cloned()
        .collect();
    let jobs = accounts
        .iter()
        .flat_map(|account| {
            state
                .sessions
                .iter()
                .flat_map(move |session| session_jobs(session, account.provider))
        })
        .collect();
    CloudPlan {
        accounts,
        jobs,
        reports_applied,
    }
}

fn session_jobs(session: &Session, provider: CloudProvider) -> Vec<CloudJob> {
    let no_remote_files = BTreeMap::new();
    let remote_files = session.remote_file_ids.get(&provider).unwrap_or(&no_remote_files);
    let wanted: BTreeSet<&String> = session
        .favourites
        .values()
        .flat_map(|favourite| favourite.file_names())
        .collect();
    let key = |file_name: &str| JobKey {
        provider,
        session_id: session.id,
        file_name: file_name.to_owned(),
    };
    let remote_folder = remote_folder_name(session);
    let uploads = wanted
        .iter()
        .filter(|file_name| !remote_files.contains_key(file_name.as_str()))
        .map(|file_name| CloudJob::Upload {
            key: key(file_name),
            remote_folder: remote_folder.clone(),
            local_path: session.dir.join(file_name),
        });
    let deletions = remote_files
        .iter()
        .filter(|(file_name, _)| !wanted.contains(file_name))
        .map(|(file_name, remote_id)| CloudJob::Delete {
            key: key(file_name),
            remote_id: remote_id.clone(),
        });
    uploads.chain(deletions).collect()
}

/// From the session name, not its local folder: two sessions may both live in folders called "Favourites".
fn remote_folder_name(session: &Session) -> String {
    session_name_to_folder_name(&session.name).unwrap_or_else(|_| session.id.0.to_string())
}

/// Returns the connect failure, if that's what the report was, for the UI to show next to its provider.
pub fn apply_cloud_report(store: &mut Store, report: CloudReport) -> AppResult<Option<(CloudProvider, String)>> {
    let state = &mut store.state;
    match report {
        CloudReport::Uploaded { key, remote_id } => {
            // Disconnected or forgotten while the upload ran: the file stays in the cloud, untracked.
            if state.cloud_account(key.provider).is_none() {
                return Ok(None);
            }
            let Some(session) = state.sessions.iter_mut().find(|session| session.id == key.session_id) else {
                return Ok(None);
            };
            session
                .remote_file_ids
                .entry(key.provider)
                .or_default()
                .insert(key.file_name, remote_id);
        }
        CloudReport::Deleted { key } => {
            let Some(remote_files) = state
                .sessions
                .iter_mut()
                .find(|session| session.id == key.session_id)
                .and_then(|session| session.remote_file_ids.get_mut(&key.provider))
            else {
                return Ok(None);
            };
            remote_files.remove(&key.file_name);
        }
        CloudReport::Connected(account) => {
            let previous = state.cloud_accounts.iter().position(|a| a.provider == account.provider);
            // A different account doesn't hold the files the old one did: upload everything to it.
            let same_account = previous.is_some_and(|index| {
                let previous = &state.cloud_accounts[index];
                previous.account_label == account.account_label && previous.webdav == account.webdav
            });
            if !same_account {
                forget_remote_files(state, account.provider);
            }
            match previous {
                Some(index) => state.cloud_accounts[index] = account,
                None => state.cloud_accounts.push(account),
            }
        }
        CloudReport::ConnectFailed { provider, error } => return Ok(Some((provider, error))),
        CloudReport::Problem { .. } => return Ok(None),
    }
    store.save()?;
    Ok(None)
}

/// Uploads stop; everything already uploaded stays where it is.
pub fn set_cloud_uploads_enabled(store: &mut Store, provider: CloudProvider, enabled: bool) -> AppResult<()> {
    let account = store
        .state
        .cloud_accounts
        .iter_mut()
        .find(|account| account.provider == provider)
        .ok_or_else(|| AppError::Invalid(format!("Connect {} first", provider.label())))?;
    account.enabled = enabled;
    store.save()
}

/// Files already uploaded stay in the cloud; the app just stops tracking them.
pub fn disconnect_cloud(store: &mut Store, provider: CloudProvider) -> AppResult<()> {
    store
        .state
        .cloud_accounts
        .retain(|account| account.provider != provider);
    forget_remote_files(&mut store.state, provider);
    store.save()
}

fn forget_remote_files(state: &mut PersistedState, provider: CloudProvider) {
    for session in &mut state.sessions {
        session.remote_file_ids.remove(&provider);
    }
}

pub fn cloud_statuses(
    state: &PersistedState,
    plan: &CloudPlan,
    problems: &BTreeMap<CloudProvider, CloudProblem>,
    can_connect: impl Fn(CloudProvider) -> bool,
) -> Vec<CloudStatus> {
    CloudProvider::ALL
        .into_iter()
        .map(|provider| CloudStatus {
            provider,
            can_connect: can_connect(provider),
            account: state.cloud_account(provider).cloned(),
            pending: plan.jobs.iter().filter(|job| job.key().provider == provider).count(),
            problem: problems.get(&provider).cloned(),
        })
        .collect()
}
