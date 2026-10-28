//! One Tauri command per `Backend` method (`app/ui/src/ipc/backend.ts`), snake_case, with the
//! DTOs of `lustro_core::dto` as arguments and results. Errors are short user-facing strings that
//! never contain a path.
//!
//! Validation: node ids must exist in the current tree; paths must be a volume, a folder the
//! user picked with the native dialog, a root scanned before, or a path of the current tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use lustro_core::cleanup::{self, ProtectedPaths};
use lustro_core::dto::{
    self, ApplyRequest, ApplyResult, CloudProviderSummary, CloudRootInfo, Decision, Destination,
    DiffDto, Filter, NodeDetailDto, NodeDto, Platform, ScanDone, ScanRequest, ScanUpdate, Settings,
    SnapshotMeta, SortKey, SpaceBreakdownDto, SuggestionDto, TreeDto, UndoEntry, UpdateStatus,
    VolumeInfo,
};
use lustro_core::model::NodeId;
use lustro_core::query::{self, TopKind};
use lustro_core::snapshot::{SaveOptions, VolumeUsage};
use lustro_core::tree::Tree;
use lustro_platform::PlatformTrash;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::scan::{name_root, FinishContext, ScanEnv, ScanEvents, Session};
use crate::state::AppState;
use crate::updates;
use crate::util::{is_within, now, path_key, same_path, ui_path};

type Res<T> = Result<T, String>;

const MAX_LIST: usize = 100_000;

// ---------------------------------------------------------------------------------------------
// Helpers

fn session(state: &AppState) -> Res<Arc<Session>> {
    state.scans.current().ok_or_else(|| "No scan yet".into())
}

fn known(tree: &Tree, id: NodeId) -> Res<()> {
    tree.get(id)
        .map(|_| ())
        .ok_or_else(|| "Unknown item".into())
}

/// Node id of `path` in `tree`, if the tree contains it.
pub fn tree_node_at(tree: &Tree, path: &Path) -> Option<NodeId> {
    let root = tree.root_display()?;
    if !is_within(path, &root) {
        return None;
    }
    let (rk, pk) = (path_key(&root)?, path_key(path)?);
    let rest = pk.get(rk.len()..)?.trim_start_matches('/');
    if rest.is_empty() {
        return tree.root_id();
    }
    // Walk by names case-insensitively where the file system is (keys are lower-cased).
    let mut cur = tree.root_id()?;
    for part in rest.split('/') {
        let n = tree.get(cur)?;
        cur = n.children().iter().copied().find(|&c| {
            tree.get(c).is_some_and(|x| {
                let name = x.name().to_string_lossy();
                if cfg!(any(windows, target_os = "macos")) {
                    name.to_lowercase() == part
                } else {
                    name == part
                }
            })
        })?;
    }
    Some(cur)
}

fn volumes(state: &AppState) -> Vec<VolumeInfo> {
    let mut list: Vec<VolumeInfo> = lustro_platform::list_volumes()
        .into_iter()
        .map(|v| VolumeInfo {
            id: v.id,
            label: v.label,
            mount_path: v.mount_path,
            file_system: v.file_system,
            total_bytes: v.total_bytes,
            free_bytes: v.free_bytes,
            is_remote: v.is_remote,
            is_removable: v.is_removable,
        })
        .collect();
    if let Some(dev) = &state.dev_root {
        let host = containing(&list, dev).cloned();
        list.insert(
            0,
            VolumeInfo {
                id: "dev-test-folder".into(),
                label: "Test folder".into(),
                mount_path: dev.to_string_lossy().into_owned(),
                file_system: host
                    .as_ref()
                    .map(|h| h.file_system.clone())
                    .unwrap_or_default(),
                total_bytes: host.as_ref().map_or(0, |h| h.total_bytes),
                free_bytes: host.as_ref().map_or(0, |h| h.free_bytes),
                is_remote: false,
                is_removable: false,
            },
        );
    }
    list
}

/// The real volume holding `p` (longest mount path that contains it).
fn containing<'a>(list: &'a [VolumeInfo], p: &Path) -> Option<&'a VolumeInfo> {
    list.iter()
        .filter(|v| v.id != "dev-test-folder" && is_within(p, Path::new(&v.mount_path)))
        .max_by_key(|v| v.mount_path.len())
}

/// May `root` be scanned? A volume, a picked folder, a root scanned before, or the dev root.
fn allowed_root(state: &AppState, root: &Path) -> bool {
    if state.was_picked(root)
        || state
            .dev_root
            .as_deref()
            .is_some_and(|d| same_path(d, root))
    {
        return true;
    }
    if lustro_platform::list_volumes()
        .iter()
        .any(|v| same_path(Path::new(&v.mount_path), root))
    {
        return true;
    }
    state
        .with_store(|s| s.roots().map_err(|_| String::new()))
        .unwrap_or_default()
        .iter()
        .any(|r| same_path(Path::new(r), root))
}

/// A new path for Settings' excluded / protected lists: picked, or a path of the current tree.
fn allowed_setting_path(state: &AppState, s: &str) -> bool {
    let Ok(p) = ui_path(s) else { return false };
    state.was_picked(&p)
        || state
            .scans
            .current()
            .is_some_and(|ses| ses.read(|t| tree_node_at(t, &p).is_some()))
}

fn cloud_roots() -> Vec<CloudRootInfo> {
    use lustro_cloud::SyncRootKind as K;
    let roots = lustro_cloud::list_sync_roots();
    let mut out: Vec<CloudRootInfo> = Vec::new();
    for r in roots.iter().filter(|r| r.kind != K::Cache) {
        let virtual_volume_id = (r.kind == K::VirtualDrive).then(|| {
            let s = r.root_path.to_string_lossy();
            s.trim_end_matches(['\\', '/']).to_string()
        });
        out.push(CloudRootInfo {
            provider: r.provider,
            display_name: r.display_name.clone(),
            root_path: r.root_path.clone(),
            remote: matches!(r.kind, K::VirtualDrive | K::RemoteMount),
            virtual_volume_id,
            cache_root: None,
        });
    }
    // A provider cache belongs to that provider's virtual drive, if it has one.
    for c in roots.iter().filter(|r| r.kind == K::Cache) {
        match out
            .iter_mut()
            .find(|o| o.provider == c.provider && o.virtual_volume_id.is_some())
        {
            Some(o) => o.cache_root = Some(c.root_path.clone()),
            None => out.push(CloudRootInfo {
                provider: c.provider,
                display_name: c.display_name.clone(),
                root_path: c.root_path.clone(),
                remote: false,
                virtual_volume_id: None,
                cache_root: None,
            }),
        }
    }
    out
}

/// The bundled fast-scan helper beside the app exe (absolute path, never PATH or the current
/// folder), with the hash baked in at build time. `None` when this build has no helper.
#[cfg(windows)]
fn helper_exe() -> Option<lustro_scan::HelperExe> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    lustro_scan::HelperExe::new(
        dir.join(lustro_scan::helper::HELPER_EXE),
        env!("LUSTRO_HELPER_SHA256"),
    )
}

#[cfg(not(windows))]
fn helper_exe() -> Option<lustro_scan::HelperExe> {
    None
}

struct AppEvents(AppHandle);

impl ScanEvents for AppEvents {
    fn update(&self, u: &ScanUpdate) {
        let _ = self.0.emit("scan-update", u);
    }
    fn done(&self, d: &ScanDone) {
        let _ = self.0.emit("scan-done", d);
    }
}

/// The suggestions cleanup works from: the current scan's, else the last saved ones.
fn current_suggestions(state: &AppState) -> Vec<SuggestionDto> {
    if let Some(s) = state.scans.current() {
        if let Some(list) = &s
            .extra
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .suggestions
        {
            return list.clone();
        }
    }
    state
        .saved_suggestions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Drops suggestions that were applied or kept.
fn forget_suggestions(state: &AppState, gone: &dyn Fn(&SuggestionDto) -> bool) {
    if let Some(s) = state.scans.current() {
        let mut x = s.extra.write().unwrap_or_else(|e| e.into_inner());
        if let Some(list) = x.suggestions.as_mut() {
            list.retain(|i| !gone(i));
        }
    }
    let mut saved = current_suggestions(state);
    saved.retain(|i| !gone(i));
    state.save_suggestions(&saved);
}

// ---------------------------------------------------------------------------------------------
// Scan

#[tauri::command]
pub fn platform() -> Platform {
    Platform::current()
}

#[tauri::command]
pub async fn list_volumes(state: State<'_, AppState>) -> Res<Vec<VolumeInfo>> {
    Ok(volumes(&state))
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    req: ScanRequest,
) -> Res<String> {
    let root = ui_path(&req.root)?;
    if !allowed_root(&state, &root) {
        return Err("Pick the folder to scan first".into());
    }
    let settings = state.settings.get();
    for x in &req.excluded {
        if !settings.excluded.iter().any(|e| e == x) {
            return Err("Excluded folders must come from Settings".into());
        }
    }
    let vols = volumes(&state);
    let usage = containing(&vols, &root).map(|v| VolumeUsage {
        used_bytes: v.total_bytes.saturating_sub(v.free_bytes),
        total_bytes: v.total_bytes,
    });
    let keep = settings.keep_snapshots;
    let retention = settings.snapshots_to_keep as usize;
    let saver = app.clone();
    let suggester = app.clone();
    let finish = FinishContext {
        save: Box::new(move |tree, usage| {
            if !keep {
                return (None, None);
            }
            let st = saver.state::<AppState>();
            st.with_store(|s| {
                s.set_retention(retention);
                let root = tree
                    .root_display()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let prev = s
                    .list(&root)
                    .ok()
                    .and_then(|l| l.into_iter().max_by_key(|m| m.taken_at));
                let meta = s
                    .save(tree, now(), usage, SaveOptions::default())
                    .map_err(|_| String::new())?;
                let deltas = prev.and_then(|p| s.deltas(tree, &p.id).ok());
                Ok((Some(meta.id), deltas))
            })
            .unwrap_or((None, None))
        }),
        kept: settings.kept_items.clone(),
        duplicates: lustro_core::insights::DuplicateOptions::from_settings(&settings),
        on_suggestions: Box::new(move |list| {
            suggester.state::<AppState>().save_suggestions(list);
        }),
    };
    state.scans.start(
        &req,
        Box::new(lustro_cloud::Classifier::detect()),
        cloud_roots(),
        ScanEnv {
            usage,
            helper: helper_exe(),
        },
        Arc::new(AppEvents(app.clone())),
        finish,
    )
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> Res<()> {
    state.scans.cancel(&scan_id)
}

// ---------------------------------------------------------------------------------------------
// Tree queries

#[tauri::command]
pub async fn root(state: State<'_, AppState>) -> Res<Option<NodeDto>> {
    let Some(s) = state.scans.current() else {
        return Ok(None);
    };
    let d = s.deltas();
    Ok(s.read(|t| {
        t.root().map(|n| {
            let mut v = [dto::node_dto(n, d.as_ref())];
            name_root(t, &mut v);
            let [v] = v;
            v
        })
    }))
}

#[tauri::command]
pub async fn children(
    state: State<'_, AppState>,
    id: NodeId,
    sort: SortKey,
    offset: usize,
    limit: usize,
) -> Res<Vec<NodeDto>> {
    let s = session(&state)?;
    let d = s.deltas();
    s.read(|t| {
        known(t, id)?;
        Ok(query::children(
            t,
            id,
            sort,
            offset,
            limit.min(MAX_LIST),
            d.as_ref(),
        ))
    })
}

#[tauri::command]
pub async fn node(state: State<'_, AppState>, id: NodeId) -> Res<NodeDetailDto> {
    let s = session(&state)?;
    let d = s.deltas();
    s.read(|t| {
        let mut x = dto::node_detail(t, id, 10, d.as_ref()).ok_or("Unknown item")?;
        name_root(t, std::slice::from_mut(&mut x.node));
        Ok(x)
    })
}

#[tauri::command]
pub async fn path_to(state: State<'_, AppState>, id: NodeId) -> Res<Vec<NodeDto>> {
    let s = session(&state)?;
    let d = s.deltas();
    s.read(|t| {
        known(t, id)?;
        let mut v = query::path_to(t, id, d.as_ref());
        name_root(t, &mut v);
        Ok(v)
    })
}

#[tauri::command]
pub async fn subtree(
    state: State<'_, AppState>,
    id: NodeId,
    max_depth: u32,
    min_fraction: f64,
) -> Res<TreeDto> {
    let s = session(&state)?;
    let d = s.deltas();
    let frac = if min_fraction.is_finite() {
        min_fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    s.read(|t| {
        let mut x =
            query::subtree(t, id, max_depth.min(64), frac, d.as_ref()).ok_or("Unknown item")?;
        if t.root_id() == Some(id) {
            x.name = t.display_path(id);
        }
        Ok(x)
    })
}

#[tauri::command]
pub async fn top(
    state: State<'_, AppState>,
    kind: String,
    n: usize,
    filter: Filter,
) -> Res<Vec<NodeDto>> {
    let kind = match kind.as_str() {
        "files" => TopKind::Files,
        "dirs" => TopKind::Dirs,
        _ => return Err("Unknown list".into()),
    };
    let s = session(&state)?;
    let d = s.deltas();
    Ok(s.read(|t| query::top(t, kind, n.min(MAX_LIST), &filter, now(), d.as_ref())))
}

#[tauri::command]
pub async fn search(state: State<'_, AppState>, filter: Filter, limit: usize) -> Res<Vec<NodeDto>> {
    let s = session(&state)?;
    let d = s.deltas();
    Ok(s.read(|t| query::search(t, &filter, limit.min(MAX_LIST), now(), d.as_ref())))
}

#[tauri::command]
pub async fn space_breakdown(
    state: State<'_, AppState>,
    volume_id: String,
) -> Res<SpaceBreakdownDto> {
    let vols = volumes(&state);
    let v = vols
        .iter()
        .find(|v| v.id == volume_id)
        .ok_or("Unknown drive")?;
    let mount = PathBuf::from(&v.mount_path);
    // The dev folder is part of a real volume: use that one's system items.
    let host = if v.id == "dev-test-folder" {
        containing(&vols, &mount)
            .map(|h| PathBuf::from(&h.mount_path))
            .unwrap_or_else(|| mount.clone())
    } else {
        mount.clone()
    };
    let used = v.total_bytes.saturating_sub(v.free_bytes);
    let system = tauri::async_runtime::spawn_blocking(move || {
        lustro_platform::system_space::system_items(&host)
    })
    .await
    .unwrap_or_default();
    let session = state.scans.current().filter(|s| !s.is_running());
    let on_volume = session.filter(|s| is_within(&s.root, &mount));
    Ok(match on_volume {
        Some(s) => {
            let whole = v.id != "dev-test-folder" && same_path(&s.root, &mount);
            s.read(|t| crate::space::breakdown(&volume_id, used, Some((t, whole)), &system))
        }
        None => crate::space::breakdown(&volume_id, used, None, &system),
    })
}

#[tauri::command]
pub async fn cloud_summary(state: State<'_, AppState>) -> Res<Vec<CloudProviderSummary>> {
    let hidden = state.settings.get().hidden_providers;
    let Some(s) = state.scans.current() else {
        return Ok(Vec::new());
    };
    let mut list = s.read(|t| dto::cloud_summary(t, &s.cloud_roots));
    list.retain(|c| !hidden.contains(&c.provider));
    Ok(list)
}

// ---------------------------------------------------------------------------------------------
// Snapshots

#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>, root: String) -> Res<Vec<SnapshotMeta>> {
    if root.len() > 4096 {
        return Err("Invalid path".into());
    }
    // Oldest first, as the UI (and the mock) expect; the store lists newest first.
    let mut list = state.with_store(|s| {
        s.list(&root)
            .map_err(|_| "Couldn't read the snapshot history".into())
    })?;
    list.reverse();
    Ok(list)
}

#[tauri::command]
pub async fn diff(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    limit: usize,
) -> Res<DiffDto> {
    let current = state.scans.current().filter(|s| !s.is_running());
    state.with_store(|store| {
        let to = store.meta(&to_id).map_err(|_| "Unknown snapshot")?;
        let run = |tree: Option<&Tree>| {
            store
                .diff(&from_id, &to_id, limit.min(MAX_LIST), tree)
                .map_err(|_| "Couldn't compare these snapshots".to_string())
        };
        match current {
            Some(s)
                if s.read(|t| {
                    t.root_display()
                        .is_some_and(|r| r.to_string_lossy() == to.root)
                }) =>
            {
                s.read(|t| run(Some(t)))
            }
            _ => run(None),
        }
    })
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Res<()> {
    state.clear_history()
}

// ---------------------------------------------------------------------------------------------
// Cleanup

#[tauri::command]
pub async fn suggestions(state: State<'_, AppState>) -> Res<Vec<SuggestionDto>> {
    Ok(current_suggestions(&state))
}

/// Resets the "applying" flag however the apply ends.
struct Applying<'a>(&'a AppState);
impl Drop for Applying<'_> {
    fn drop(&mut self) {
        self.0.applying.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn apply_cleanup(state: State<'_, AppState>, req: ApplyRequest) -> Res<ApplyResult> {
    apply(&state, req).await
}

pub async fn apply(state: &AppState, req: ApplyRequest) -> Res<ApplyResult> {
    if state.scans.is_running() {
        return Err("Wait for the scan to finish".into());
    }
    if req.decisions.len() > MAX_LIST {
        return Err("Too many items".into());
    }
    if let Destination::Move { target } = &req.destination {
        let t = ui_path(target)?;
        if !state.was_picked(&t) {
            return Err("Pick the destination folder first".into());
        }
    }
    if state.applying.swap(true, Ordering::SeqCst) {
        return Err("A cleanup is already running".into());
    }
    let _guard = Applying(state);

    let list = current_suggestions(state);
    // "Keep" decisions: never suggest these again.
    let kept: Vec<String> = req
        .decisions
        .iter()
        .filter(|d| d.decision == Decision::Keep)
        .filter_map(|d| list.iter().find(|s| s.id == d.suggestion_id))
        .map(|s| s.path.clone())
        .collect();
    if !kept.is_empty() {
        state.settings.update(|s| {
            for k in &kept {
                if !s.kept_items.contains(k) {
                    s.kept_items.push(k.clone());
                }
            }
        })?;
    }

    let settings = state.settings.get();
    let mut protected = ProtectedPaths::for_this_os(lustro_platform::protected_rules());
    protected.add_user_paths(settings.protected_paths.iter().map(String::as_str));
    let plan = cleanup::plan(&list, &req, &protected);

    let result = {
        let mut log = state.undo.lock().unwrap_or_else(|e| e.into_inner());
        let ops = PlatformTrash::new();
        let r = cleanup::apply(
            &plan,
            &ops,
            &mut log,
            now(),
            &lustro_core::scan::CancelToken::new(),
        );
        state.save_undo(&log);
        r
    };

    let failed: Vec<&str> = result.failed.iter().map(|f| f.path.as_str()).collect();
    prune_tree(state, &plan, &failed);
    let done: Vec<&str> = plan
        .items
        .iter()
        .map(|i| i.suggestion_id.as_str())
        .collect();
    forget_suggestions(state, &|s| {
        kept.contains(&s.path)
            || (done.contains(&s.id.as_str()) && !failed.contains(&s.path.as_str()))
    });
    Ok(result)
}

/// Drops trashed items from the current tree (and marks freed-up cloud files cloud-only), so
/// sizes update right away. Only for items still at the same id and path in the current tree.
/// Undo doesn't re-add them; the next scan does.
fn prune_tree(state: &AppState, plan: &cleanup::ApplyPlan, failed: &[&str]) {
    let Some(s) = state.scans.current().filter(|s| !s.is_running()) else {
        return;
    };
    for item in &plan.items {
        if failed.contains(&item.path.to_string_lossy().as_ref()) {
            continue;
        }
        let here = s.read(|t| {
            t.get(item.node_id).is_some() && same_path(&t.path_of(item.node_id), &item.path)
        });
        if !here {
            continue;
        }
        match item.action {
            cleanup::Action::FreeUpCloud => {
                s.builder.mark_cloud_only(item.node_id);
            }
            _ => {
                s.builder.remove_subtree(item.node_id);
            }
        }
    }
}

#[tauri::command]
pub async fn undo_log(state: State<'_, AppState>) -> Res<Vec<UndoEntry>> {
    Ok(state
        .undo
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries())
}

#[tauri::command]
pub async fn undo(state: State<'_, AppState>, undo_id: String) -> Res<()> {
    if undo_id.len() > 64 {
        return Err("Unknown cleanup".into());
    }
    if state.applying.swap(true, Ordering::SeqCst) {
        return Err("A cleanup is already running".into());
    }
    let _guard = Applying(&state);
    let mut log = state.undo.lock().unwrap_or_else(|e| e.into_inner());
    let r = log.undo(&undo_id, &PlatformTrash::new());
    state.save_undo(&log);
    match r {
        Ok(failed) if failed.is_empty() => Ok(()),
        Ok(failed) => Err(format!(
            "{} item{} couldn't be put back",
            failed.len(),
            if failed.len() == 1 { "" } else { "s" }
        )),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// OS integration

#[tauri::command]
pub async fn reveal(state: State<'_, AppState>, id: NodeId) -> Res<()> {
    let s = session(&state)?;
    let path = s.read(|t| {
        known(t, id)?;
        Ok::<_, String>(t.path_of(id))
    })?;
    lustro_platform::reveal::reveal(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_setting(action: String) -> Res<()> {
    if !lustro_platform::system_space::setting_actions().contains(&action.as_str()) {
        return Err("Unknown setting".into());
    }
    lustro_platform::system_space::open_setting(&action).map_err(|e| e.to_string())
}

/// Native folder picker. The picked folder may then be scanned, excluded, protected or used
/// as a move destination.
#[tauri::command]
pub async fn pick_folder(window: WebviewWindow, state: State<'_, AppState>) -> Res<Option<String>> {
    let picked = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .pick_folder()
        .await;
    Ok(picked.map(|h| {
        let p = h.path().to_path_buf();
        state.add_picked(&p);
        p.to_string_lossy().into_owned()
    }))
}

// ---------------------------------------------------------------------------------------------
// Settings

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Res<Settings> {
    let mut s = state.settings.get();
    if crate::PORTABLE {
        s.auto_update = false;
    }
    Ok(s)
}

#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Res<()> {
    let turned_on = store_settings(&state, settings)?;
    if turned_on {
        tauri::async_runtime::spawn(async move {
            if let UpdateStatus::Available { .. } = updates::check(&app, false).await {
                let _ = updates::download(&app).await;
            }
        });
    }
    Ok(())
}

/// Validates and saves settings from the UI. Returns true when auto-update was just turned on.
pub fn store_settings(state: &AppState, mut new: Settings) -> Res<bool> {
    crate::settings::check_sizes(&new)?;
    let old = state.settings.get();
    for (list, old_list) in [
        (&new.excluded, &old.excluded),
        (&new.protected_paths, &old.protected_paths),
    ] {
        for p in list {
            if !old_list.contains(p) && !allowed_setting_path(state, p) {
                return Err("Pick the folder with the folder picker".into());
            }
        }
    }
    if crate::PORTABLE {
        // The portable build never updates.
        new.auto_update = false;
    }
    // Set by the backend only.
    new.last_update_check = old.last_update_check;
    if new.skipped_version.is_some() && new.skipped_version != old.skipped_version {
        new.skipped_version = old.skipped_version.clone();
    }
    let turned_on = new.auto_update && !old.auto_update;
    state.settings.update(|s| *s = new)?;
    Ok(turned_on)
}

// ---------------------------------------------------------------------------------------------
// Updates

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Res<UpdateStatus> {
    if crate::PORTABLE {
        return Err(PORTABLE_NO_UPDATER.into());
    }
    Ok(updates::check(&app, true).await)
}

/// What the update commands answer in the portable build.
const PORTABLE_NO_UPDATER: &str = "Updates are turned off in the portable version";

#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    if crate::PORTABLE {
        return Err(PORTABLE_NO_UPDATER.into());
    }
    updates::download(&app).await?;
    if state.is_busy() {
        // Stays "ready"; the user restarts once the scan or cleanup is over.
        return Ok(());
    }
    updates::install_now(&app, true)
}

/// What kind of build this is (`BuildInfo` in types.ts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// The portable zip: auto-update is off and locked.
    pub portable: bool,
}

#[tauri::command]
pub fn build_info() -> BuildInfo {
    BuildInfo {
        portable: crate::PORTABLE,
    }
}

/// The latest update status (for a page that opens after the event was sent).
#[tauri::command]
pub async fn update_status(state: State<'_, AppState>) -> Res<UpdateStatus> {
    Ok(state.updates.status())
}

#[tauri::command]
pub fn current_release_notes() -> String {
    updates::RELEASE_NOTES.to_string()
}

#[tauri::command]
pub async fn restart_to_update(app: AppHandle) -> Res<()> {
    if crate::PORTABLE {
        return Err(PORTABLE_NO_UPDATER.into());
    }
    updates::install_now(&app, true)
}

#[tauri::command]
pub async fn skip_version(app: AppHandle, state: State<'_, AppState>, version: String) -> Res<()> {
    let ok = !version.is_empty()
        && version.len() <= 32
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    if !ok {
        return Err("Invalid version".into());
    }
    state
        .settings
        .update(|s| s.skipped_version = Some(version.clone()))?;
    state.updates.skip(&app, &version);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lustro_core::dto::{DecisionItem, Destination};
    use lustro_core::fixtures::Fixture;

    #[test]
    fn finds_tree_paths_only_inside_the_root() {
        use lustro_core::fixtures::{FixturePlatform, FixtureSpec};
        // A tree of this OS: `C:\` is not an absolute path on Unix.
        let platform = if cfg!(windows) {
            FixturePlatform::Windows
        } else if cfg!(target_os = "macos") {
            FixturePlatform::Macos
        } else {
            FixturePlatform::Linux
        };
        let tree = Fixture::generate(FixtureSpec {
            platform,
            ..FixtureSpec::default()
        })
        .tree();
        let root = tree.root_display().unwrap();
        assert_eq!(tree_node_at(&tree, &root), tree.root_id());
        let some = tree.iter().find(|n| n.parent.is_some()).unwrap().id;
        let p = tree.path_of(some);
        assert_eq!(tree_node_at(&tree, &p), Some(some));
        let outside = if cfg!(windows) {
            r"Z:\Nowhere"
        } else {
            "/nowhere/x"
        };
        assert_eq!(tree_node_at(&tree, Path::new(outside)), None);
    }

    #[test]
    fn settings_reject_unpicked_paths_and_keep_backend_fields() {
        let st = AppState::in_memory();
        st.settings
            .update(|s| s.last_update_check = Some(7))
            .unwrap();
        let picked = if cfg!(windows) {
            r"C:\Users\William\Videos"
        } else {
            "/home/william/Videos"
        };

        let mut s = st.settings.get();
        s.excluded.push(picked.into());
        assert!(store_settings(&st, s.clone()).is_err());

        st.add_picked(Path::new(picked));
        s.last_update_check = None;
        s.skipped_version = Some("9.9.9".into());
        s.auto_update = false;
        assert!(!store_settings(&st, s).unwrap());
        let saved = st.settings.get();
        assert_eq!(saved.excluded, vec![picked.to_string()]);
        assert_eq!(saved.last_update_check, Some(7));
        assert_eq!(saved.skipped_version, None);

        let mut s = saved;
        s.auto_update = true;
        // The portable build refuses to turn auto-update on.
        assert_eq!(store_settings(&st, s).unwrap(), !crate::PORTABLE);
        assert_eq!(st.settings.get().auto_update, !crate::PORTABLE);
        assert_eq!(build_info().portable, crate::PORTABLE);
    }

    /// Scan, apply a temp-files suggestion to the real Recycle Bin, check the tree was
    /// pruned, then undo.
    #[cfg(windows)]
    #[test]
    fn apply_trashes_prunes_and_undoes() {
        struct Quiet;
        impl ScanEvents for Quiet {
            fn update(&self, _: &ScanUpdate) {}
            fn done(&self, _: &ScanDone) {}
        }
        let dir = tempfile::tempdir().unwrap();
        // CI's temp dir is spelled with 8.3 short names (`RUNNER~1`); the Recycle Bin records
        // long names, so restore would not find the item. The app only scans roots from the
        // volume list or the folder picker, which are long, so scan the long spelling too.
        let root = long_path(dir.path());
        std::fs::create_dir(dir.path().join("Temp")).unwrap();
        std::fs::write(dir.path().join("Temp").join("old.tmp"), vec![0u8; 11 << 20]).unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"keep").unwrap();

        let st = AppState::in_memory();
        let req = ScanRequest {
            root: root.to_string_lossy().into_owned(),
            mode: lustro_core::scan::ScanMode::Standard,
            cross_filesystems: false,
            include_remote: false,
            excluded: vec![],
        };
        let finish = FinishContext {
            save: Box::new(|_, _| (None, None)),
            kept: vec![],
            duplicates: lustro_core::insights::DuplicateOptions::default(),
            on_suggestions: Box::new(|_| {}),
        };
        st.scans
            .start(
                &req,
                Box::new(lustro_core::scan::NoCloud),
                vec![],
                ScanEnv::default(),
                Arc::new(Quiet),
                finish,
            )
            .unwrap();
        st.scans.join();
        let temp = current_suggestions(&st)
            .into_iter()
            .find(|s| s.name == "Temp")
            .expect("temp folder suggested");
        let s = st.scans.current().unwrap();
        let before = s.read(|t| t.root().unwrap().totals.allocated);

        let r = tauri::async_runtime::block_on(apply(
            &st,
            ApplyRequest {
                decisions: vec![DecisionItem {
                    suggestion_id: temp.id.clone(),
                    decision: Decision::Remove,
                }],
                destination: Destination::Trash,
            },
        ))
        .unwrap();
        assert_eq!(r.applied, 1, "{:?}", r.failed);
        assert!(!dir.path().join("Temp").exists());
        assert!(s.read(|t| t.get(temp.node_id).is_none()));
        assert!(s.read(|t| t.root().unwrap().totals.allocated) < before);
        assert!(current_suggestions(&st).iter().all(|x| x.id != temp.id));

        let failed = {
            let mut log = st.undo.lock().unwrap();
            let id = log.entries()[0].id.clone();
            log.undo(&id, &PlatformTrash::new()).unwrap()
        };
        // Reasons only: the paths are the runner's.
        let reasons: Vec<&str> = failed.iter().map(|f| f.message.as_str()).collect();
        assert!(failed.is_empty(), "undo failed: {reasons:?}");
        assert!(dir.path().join("Temp").join("old.tmp").exists());
    }

    /// `canonicalize` without the `\\?\` prefix: long names, as the folder picker returns.
    #[cfg(windows)]
    fn long_path(p: &Path) -> PathBuf {
        let c = std::fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
    }

    #[test]
    fn apply_refuses_unpicked_move_target_and_unknown_items() {
        let st = AppState::in_memory();
        let rt = tauri::async_runtime::block_on(apply(
            &st,
            ApplyRequest {
                decisions: vec![],
                destination: Destination::Move {
                    target: if cfg!(windows) {
                        r"C:\Elsewhere".into()
                    } else {
                        "/elsewhere".into()
                    },
                },
            },
        ));
        assert!(rt.is_err());

        let r = tauri::async_runtime::block_on(apply(
            &st,
            ApplyRequest {
                decisions: vec![DecisionItem {
                    suggestion_id: "tempFiles:999".into(),
                    decision: Decision::Remove,
                }],
                destination: Destination::Trash,
            },
        ))
        .unwrap();
        assert_eq!(r.applied, 0);
        assert_eq!(r.failed.len(), 1);
        assert!(!st.applying.load(Ordering::SeqCst));
    }
}
