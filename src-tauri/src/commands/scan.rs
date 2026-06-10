use crate::events::ProgressEvent;
use crate::state::{AppState, ScanAsyncStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smart_disk_cleaner_core::ai_advisor::build_local_advice;
use smart_disk_cleaner_core::analyzer::{analyze, build_scan_modules, AnalyzerOptions};
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::dedup::{find_duplicates_with_progress_and_cache, HashCache};
use smart_disk_cleaner_core::diagnostics::{probe_path, DiagnosticOperation};
use smart_disk_cleaner_core::models::{
    AnalysisResult, DedupResult, DuplicateGroup, FileRecord, FileSuggestion,
    GovernanceSuggestion, PathDiagnosis, PathIssue, ScanModuleSummary, ScanReport,
};
use smart_disk_cleaner_core::scanner::{scan_directory_with_progress_and_options, ScanOptions};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::{ipc::Channel, State};

const FRONTEND_LARGE_FILES_LIMIT: usize = 50;
const FRONTEND_TEMP_FILES_LIMIT: usize = 50;
const FRONTEND_ARCHIVE_FILES_LIMIT: usize = 50;
const FRONTEND_DUPLICATE_GROUPS_LIMIT: usize = 10;
const FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT: usize = 20;
const FRONTEND_SUGGESTIONS_LIMIT: usize = 1000;
const DIRECTORY_OVERVIEW_LIMIT: usize = 200;
const FILE_TREE_MATCH_LIMIT: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UsnJournalSnapshot {
    volume: String,
    journal_id: String,
    next_usn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedScanReport {
    config_signature: String,
    usn_snapshot: UsnJournalSnapshot,
    report: ScanReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAdvisorOutput {
    source: String,
    summary: String,
    suggestions: Vec<FileSuggestion>,
    governance_suggestions: Vec<GovernanceSuggestion>,
    suggestion_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDedupResult {
    groups: Vec<DuplicateGroup>,
    failures: Vec<PathIssue>,
    group_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendScanReport {
    generated_at: chrono::DateTime<chrono::Utc>,
    scan_duration_ms: u64,
    root: PathBuf,
    scanned_files: Vec<FileRecord>,
    analysis: AnalysisResult,
    dedup: FrontendDedupResult,
    modules: Vec<ScanModuleSummary>,
    advisor: FrontendAdvisorOutput,
    failures: Vec<PathIssue>,
    dedup_pending: bool,
    dedup_phase: Option<String>,
    dedup_message: Option<String>,
    dedup_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryOverviewItem {
    key: String,
    name: String,
    path: String,
    kind: String,
    file_count: usize,
    total_size: u64,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppOverviewItem {
    key: String,
    app_name: String,
    vendor: String,
    category: String,
    source_summary: String,
    status_tags: Vec<String>,
    icon_data_uri: String,
    icon_source: String,
    detected_root: String,
    file_count: usize,
    total_size: u64,
    sample_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    key: String,
    name: String,
    path: String,
    kind: String,
    size: u64,
    extension: String,
    file_count: usize,
    children: Option<Vec<FileTreeNode>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeQueryResult {
    matched_count: usize,
    node_count: usize,
    truncated: bool,
    rows: Vec<FileTreeNode>,
}

#[tauri::command]
pub async fn start_scan(
    path: String,
    on_progress: Channel<ProgressEvent>,
    state: State<'_, AppState>,
) -> Result<FrontendScanReport, String> {
    let root = PathBuf::from(&path);
    let started_at = Instant::now();
    let config = state.load_config();
    let cancel = state.cancel_flag.clone();
    cancel.store(false, Ordering::Relaxed);

    if let Some(cached_report) = try_restore_cached_report(&root, &config, &state) {
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "cached".to_string(),
            detail: "检测到卷无变化，直接复用上次扫描结果。".to_string(),
        });
        *state.last_report.lock().await = Some(cached_report.clone());
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "done".to_string(),
            detail: "扫描完成".to_string(),
        });
        return Ok(summarize_report_for_frontend(&cached_report, None));
    }

    let cancel_scan = cancel.clone();
    let on_progress_scan = on_progress.clone();
    let scan_root = root.clone();
    let exclude_patterns = config.exclude_patterns.clone();

    let scan = tokio::task::spawn_blocking(move || {
        scan_directory_with_progress_and_options(
            &scan_root,
            |progress| {
                let _ = on_progress_scan.send(ProgressEvent::from(progress));
            },
            cancel_scan,
            &ScanOptions { exclude_patterns },
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "hash_cache".to_string(),
        detail: "正在分析文件类型和大小...".to_string(),
    });
    let files = scan.files.clone();
    let cancel_dedup = cancel.clone();
    let on_progress_dedup = on_progress.clone();
    let hash_cache = load_hash_cache(&state, &root).unwrap_or_default();
    let dedup_root = root.clone();
    let (dedup, updated_hash_cache) = tokio::task::spawn_blocking(move || {
        find_duplicates_with_progress_and_cache(
            &files,
            &hash_cache,
            |progress| {
                let _ = on_progress_dedup.send(ProgressEvent::from(progress));
            },
            cancel_dedup,
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    save_hash_cache(&state, &dedup_root, &updated_hash_cache).map_err(|error| error.to_string())?;

    let analyze_started_at = Instant::now();
    let analysis = analyze(
        &scan,
        AnalyzerOptions {
            large_file_threshold_bytes: config.large_file_threshold_mb * 1024 * 1024,
        },
    );
    let analyze_elapsed_ms = analyze_started_at.elapsed().as_millis();

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "type_breakdown".to_string(),
        detail: format!("正在统计文件类型分布，已完成基础分析（{} ms）...", analyze_elapsed_ms),
    });

    let module_started_at = Instant::now();
    let modules = build_scan_modules(&analysis, &dedup);
    let module_elapsed_ms = module_started_at.elapsed().as_millis();

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "advising".to_string(),
        detail: format!("正在生成清理建议（模块汇总耗时 {} ms）...", module_elapsed_ms),
    });

    let advice_started_at = Instant::now();
    let advisor = build_local_advice(&analysis, &dedup);
    let advice_elapsed_ms = advice_started_at.elapsed().as_millis();

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "packaging".to_string(),
        detail: format!("正在整理结果给界面展示（建议生成耗时 {} ms）...", advice_elapsed_ms),
    });

    let report = ScanReport {
        generated_at: Utc::now(),
        scan_duration_ms: started_at.elapsed().as_millis() as u64,
        root: scan.root.clone(),
        scanned_files: scan.files.clone(),
        modules,
        analysis,
        dedup,
        advisor,
        failures: scan.failures,
    };

    store_cached_report(&root, &config, &state, &report).map_err(|error| error.to_string())?;
    *state.last_report.lock().await = Some(report.clone());

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "caching_report".to_string(),
        detail: "扫描完成".to_string(),
    });

    let summarize_started_at = Instant::now();
    let frontend_report = summarize_report_for_frontend(
        &report,
        Some(&state.scan_async_status.lock().await.clone()),
    );
    let summarize_elapsed_ms = summarize_started_at.elapsed().as_millis();

    eprintln!(
        "[scan-analysis] root={} analyze={}ms modules={}ms advice={}ms summarize={}ms",
        root.display(),
        analyze_elapsed_ms,
        module_elapsed_ms,
        advice_elapsed_ms,
        summarize_elapsed_ms
    );

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "done".to_string(),
        detail: format!(
            "扫描完成。基础分析 {} ms，模块汇总 {} ms，建议生成 {} ms，结果整理 {} ms。",
            analyze_elapsed_ms, module_elapsed_ms, advice_elapsed_ms, summarize_elapsed_ms
        ),
    });

    Ok(frontend_report)
}

#[tauri::command]
pub async fn start_scan_v2(
    path: String,
    on_progress: Channel<ProgressEvent>,
    state: State<'_, AppState>,
) -> Result<FrontendScanReport, String> {
    let root = PathBuf::from(&path);
    let started_at = Instant::now();
    let config = state.load_config();
    let cancel = state.cancel_flag.clone();
    cancel.store(false, Ordering::Relaxed);

    if let Some(cached_report) = try_restore_cached_report(&root, &config, &state) {
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "cached".to_string(),
            detail: "检测到磁盘内容未发生变化，直接复用上次扫描结果。".to_string(),
        });
        *state.last_report.lock().await = Some(cached_report.clone());
        {
            let mut status = state.scan_async_status.lock().await;
            *status = ScanAsyncStatus::default();
        }
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "done".to_string(),
            detail: "扫描完成".to_string(),
        });
        return Ok(summarize_report_for_frontend(&cached_report, None));
    }

    let scan_id = state.scan_epoch.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let mut status = state.scan_async_status.lock().await;
        *status = ScanAsyncStatus::default();
    }

    let cancel_scan = cancel.clone();
    let on_progress_scan = on_progress.clone();
    let scan_root = root.clone();
    let exclude_patterns = config.exclude_patterns.clone();

    let scan = tokio::task::spawn_blocking(move || {
        scan_directory_with_progress_and_options(
            &scan_root,
            |progress| {
                let _ = on_progress_scan.send(ProgressEvent::from(progress));
            },
            cancel_scan,
            &ScanOptions { exclude_patterns },
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    let analyze_started_at = Instant::now();
    let analysis = analyze(
        &scan,
        AnalyzerOptions {
            large_file_threshold_bytes: config.large_file_threshold_mb * 1024 * 1024,
        },
    );
    let analyze_elapsed_ms = analyze_started_at.elapsed().as_millis();

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "type_breakdown".to_string(),
        detail: format!("正在统计文件类型分布，基础分析耗时 {} ms。", analyze_elapsed_ms),
    });

    let empty_dedup = DedupResult {
        groups: Vec::new(),
        failures: Vec::new(),
    };

    let module_started_at = Instant::now();
    let modules = build_scan_modules(&analysis, &empty_dedup);
    let module_elapsed_ms = module_started_at.elapsed().as_millis();

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "advising".to_string(),
        detail: format!("正在生成基础清理建议，模块汇总耗时 {} ms。", module_elapsed_ms),
    });

    let advice_started_at = Instant::now();
    let advisor = build_local_advice(&analysis, &empty_dedup);
    let advice_elapsed_ms = advice_started_at.elapsed().as_millis();

    let generated_at = Utc::now();
    let scan_duration_ms = started_at.elapsed().as_millis() as u64;
    let partial_report = ScanReport {
        generated_at,
        scan_duration_ms,
        root: scan.root.clone(),
        scanned_files: scan.files.clone(),
        modules,
        analysis,
        dedup: empty_dedup,
        advisor,
        failures: scan.failures,
    };

    *state.last_report.lock().await = Some(partial_report);
    {
        let mut status = state.scan_async_status.lock().await;
        *status = ScanAsyncStatus {
            dedup_pending: true,
            phase: "background_dedup".to_string(),
            message: "基础结果已可查看，重复文件识别正在后台继续加载。".to_string(),
            error: None,
        };
    }

    let summarize_started_at = Instant::now();
    let current_status = state.scan_async_status.lock().await.clone();
    let frontend_report = {
        let report_guard = state.last_report.lock().await;
        let report = report_guard
            .as_ref()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
        summarize_report_for_frontend(report, Some(&current_status))
    };
    let summarize_elapsed_ms = summarize_started_at.elapsed().as_millis();

    eprintln!(
        "[scan-analysis] root={} analyze={}ms modules={}ms advice={}ms summarize={}ms",
        root.display(),
        analyze_elapsed_ms,
        module_elapsed_ms,
        advice_elapsed_ms,
        summarize_elapsed_ms
    );

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "done".to_string(),
        detail: format!(
            "基础结果已生成。基础分析 {} ms，模块汇总 {} ms，基础建议 {} ms，结果整理 {} ms。",
            analyze_elapsed_ms, module_elapsed_ms, advice_elapsed_ms, summarize_elapsed_ms
        ),
    });

    let last_report = state.last_report.clone();
    let async_status = state.scan_async_status.clone();
    let scan_epoch = state.scan_epoch.clone();
    let cancel_dedup = cancel.clone();
    let cache_dir = cache_dir(&state);
    let root_for_task = root.clone();
    let files_for_task = scan.files.clone();
    let analysis_for_task = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .map(|report| report.analysis.clone())
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    let failures_for_task = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .map(|report| report.failures.clone())
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    let config_for_task = config.clone();
    let report_root = root.clone();

    tokio::spawn(async move {
        {
            let mut status = async_status.lock().await;
            *status = ScanAsyncStatus {
                dedup_pending: true,
                phase: "hashing_duplicates".to_string(),
                message: "重复文件识别正在后台执行，可先查看大文件、临时文件和基础建议。".to_string(),
                error: None,
            };
        }

        let hash_cache = load_hash_cache_from_dir(&cache_dir, &root_for_task).unwrap_or_default();
        let files_for_dedup = files_for_task.clone();
        let dedup_result = tokio::task::spawn_blocking(move || {
            find_duplicates_with_progress_and_cache(
                &files_for_dedup,
                &hash_cache,
                |_| {},
                cancel_dedup,
            )
        })
        .await;

        let (dedup, updated_hash_cache) = match dedup_result {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                let mut status = async_status.lock().await;
                *status = ScanAsyncStatus {
                    dedup_pending: false,
                    phase: "failed".to_string(),
                    message: "重复文件识别失败，当前仅展示基础扫描结果。".to_string(),
                    error: Some(error.to_string()),
                };
                return;
            }
            Err(error) => {
                let mut status = async_status.lock().await;
                *status = ScanAsyncStatus {
                    dedup_pending: false,
                    phase: "failed".to_string(),
                    message: "重复文件识别线程执行失败，当前仅展示基础扫描结果。".to_string(),
                    error: Some(error.to_string()),
                };
                return;
            }
        };

        if scan_epoch.load(Ordering::Relaxed) != scan_id {
            return;
        }

        {
            let mut status = async_status.lock().await;
            *status = ScanAsyncStatus {
                dedup_pending: true,
                phase: "merging_results".to_string(),
                message: "重复文件已识别完成，正在合并到结果页。".to_string(),
                error: None,
            };
        }

        let modules = build_scan_modules(&analysis_for_task, &dedup);
        let advisor = build_local_advice(&analysis_for_task, &dedup);
        let full_report = ScanReport {
            generated_at,
            scan_duration_ms,
            root: report_root,
            scanned_files: files_for_task,
            analysis: analysis_for_task,
            dedup,
            modules,
            advisor,
            failures: failures_for_task,
        };

        if scan_epoch.load(Ordering::Relaxed) != scan_id {
            return;
        }

        *last_report.lock().await = Some(full_report.clone());
        {
            let mut status = async_status.lock().await;
            *status = ScanAsyncStatus {
                dedup_pending: false,
                phase: "done".to_string(),
                message: "重复文件结果已补充到结果页。".to_string(),
                error: None,
            };
        }

        let cache_dir_for_write = cache_dir.clone();
        let root_for_write = root_for_task.clone();
        let report_for_write = full_report;
        tokio::task::spawn_blocking(move || {
            let _ = save_hash_cache_to_dir(&cache_dir_for_write, &root_for_write, &updated_hash_cache);
            let _ = store_cached_report_to_dir(
                &cache_dir_for_write,
                &root_for_write,
                &config_for_task,
                &report_for_write,
            );
        })
        .await
        .ok();
    });

    Ok(frontend_report)
}

#[tauri::command]
pub async fn get_latest_scan_report(
    state: State<'_, AppState>,
) -> Result<FrontendScanReport, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    let status = state.scan_async_status.lock().await.clone();
    Ok(summarize_report_for_frontend(&report, Some(&status)))
}

#[tauri::command]
pub async fn get_directory_overview(
    state: State<'_, AppState>,
) -> Result<Vec<DirectoryOverviewItem>, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
    Ok(build_directory_overview(report, DIRECTORY_OVERVIEW_LIMIT))
}

#[tauri::command]
pub async fn get_app_overview(state: State<'_, AppState>) -> Result<Vec<AppOverviewItem>, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
    Ok(build_app_overview(report, 24))
}

#[tauri::command]
pub async fn query_file_tree(
    query: Option<String>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<FileTreeQueryResult, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
    Ok(build_file_tree_query_result(
        report,
        query.as_deref().unwrap_or_default(),
        category.as_deref().unwrap_or("all"),
        FILE_TREE_MATCH_LIMIT,
    ))
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn diagnose_path(path: String, operation: String) -> Result<PathDiagnosis, String> {
    let op = match operation.as_str() {
        "recycle" => DiagnosticOperation::Recycle,
        "move" => DiagnosticOperation::Move,
        _ => DiagnosticOperation::Probe,
    };
    Ok(probe_path(&PathBuf::from(path), op))
}

#[tauri::command]
pub async fn get_directory_overview_v2(
    state: State<'_, AppState>,
) -> Result<Vec<DirectoryOverviewItem>, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    Ok(build_directory_overview(&report, DIRECTORY_OVERVIEW_LIMIT))
}

#[tauri::command]
pub async fn get_app_overview_v2(
    state: State<'_, AppState>,
) -> Result<Vec<AppOverviewItem>, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    Ok(build_app_overview(&report, 24))
}

#[tauri::command]
pub async fn query_file_tree_v2(
    query: Option<String>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<FileTreeQueryResult, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    Ok(build_file_tree_query_result(
        &report,
        query.as_deref().unwrap_or_default(),
        category.as_deref().unwrap_or("all"),
        FILE_TREE_MATCH_LIMIT,
    ))
}

#[tauri::command]
pub async fn query_directory_tree_v2(
    query: Option<String>,
    state: State<'_, AppState>,
) -> Result<FileTreeQueryResult, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    Ok(build_directory_tree_query_result(
        &report,
        query.as_deref().unwrap_or_default(),
        120,
    ))
}

/// 增量加载指定目录下的直接子节点（文件和子目录）
#[tauri::command]
pub async fn load_directory_children(
    dir_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileTreeNode>, String> {
    let report = {
        let report_guard = state.last_report.lock().await;
        report_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?
    };
    
    Ok(build_directory_children(&report, &dir_path))
}

fn build_directory_children(report: &ScanReport, dir_path: &str) -> Vec<FileTreeNode> {
    // 分隔路径
    let target_parts: Vec<&str> = if dir_path.is_empty() {
        vec![]
    } else {
        dir_path.split('/').collect()
    };
    
    // 记录当前目录下的直接子项{文件名:(是否是目录，大小，扩展名，完整路径)}
    let mut children_map: HashMap<String, (bool, u64, String, String)> = HashMap::new();
    
    for file in &report.scanned_files {
        //空文件不显示，无意义
        if file.size == 0 {
            continue;
        }
        
        let parts = relative_parts(&file.path, &report.root);
        if parts.is_empty() {
            continue;
        }
        
        if parts.len() <= target_parts.len() {
            continue;
        }
        
        // 匹配前缀
        let prefix_matches = target_parts.iter().enumerate().all(|(i, target)| {
            parts.get(i).map_or(false, |part| part == *target)
        });
        
        if !prefix_matches {
            continue;
        }
        
        let child_name = &parts[target_parts.len()];
        let is_direct_child = parts.len() == target_parts.len() + 1;
        
        if is_direct_child {
            // 直接子文件，直接添加
            let entry = children_map.entry(child_name.to_string()).or_insert_with(|| {
                (
                    false,
                    0,
                    file.extension.clone().unwrap_or_default(),
                    file.path.to_string_lossy().to_string(),
                )
            });
            entry.1 += file.size;
        } else {
            // 直接子文件，累加文件大小
            let entry = children_map.entry(child_name.to_string()).or_insert_with(|| {
                (
                    true,
                    0,
                    String::new(),
                    report.root.join(parts[..=target_parts.len()].join("/")).to_string_lossy().to_string(),
                )
            });
            entry.1 += file.size;
        }
    }
    
    // 转换为 FileTreeNode 列表
    let mut nodes: Vec<FileTreeNode> = children_map
        .into_iter()
        .map(|(name, (is_dir, size, ext, path))| {
            let key = if is_dir {
                format!("dir:{}{}{}", dir_path, if dir_path.is_empty() { "" } else { "/" }, name)
            } else {
                format!("file:{}{}{}", dir_path, if dir_path.is_empty() { "" } else { "/" }, name)
            };
            
            FileTreeNode {
                key,
                name,
                path,
                kind: if is_dir { "directory".to_string() } else { "file".to_string() },
                size,
                extension: ext,
                file_count: if is_dir { 0 } else { 1 },
                children: None,
            }
        })
        .collect();
    
    // 排序：目录在前，然后按大小降序，最后按名称升序
    nodes.sort_by(|left, right| {
        if left.kind != right.kind {
            return if left.kind == "directory" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if left.size != right.size {
            return right.size.cmp(&left.size);
        }
        left.name.cmp(&right.name)
    });
    
    nodes
}

fn summarize_report_for_frontend(
    report: &ScanReport,
    async_status: Option<&ScanAsyncStatus>,
) -> FrontendScanReport {
    let (dedup_pending, dedup_phase, dedup_message, dedup_error) = async_status
        .map(|status| {
            (
                status.dedup_pending,
                if status.phase.is_empty() {
                    None
                } else {
                    Some(status.phase.clone())
                },
                if status.message.is_empty() {
                    None
                } else {
                    Some(status.message.clone())
                },
                status.error.clone(),
            )
        })
        .unwrap_or((false, None, None, None));

    FrontendScanReport {
        generated_at: report.generated_at,
        scan_duration_ms: report.scan_duration_ms,
        root: report.root.clone(),
        scanned_files: Vec::new(),
        analysis: AnalysisResult {
            total_files: report.analysis.total_files,
            total_size: report.analysis.total_size,
            empty_files: report
                .analysis
                .empty_files
                .iter()
                .take(100)
                .cloned()
                .collect(),
            empty_dirs: report
                .analysis
                .empty_dirs
                .iter()
                .take(100)
                .cloned()
                .collect(),
            large_files: report
                .analysis
                .large_files
                .iter()
                .take(FRONTEND_LARGE_FILES_LIMIT)
                .cloned()
                .collect(),
            temporary_files: report
                .analysis
                .temporary_files
                .iter()
                .take(FRONTEND_TEMP_FILES_LIMIT)
                .cloned()
                .collect(),
            archive_files: report
                .analysis
                .archive_files
                .iter()
                .take(FRONTEND_ARCHIVE_FILES_LIMIT)
                .cloned()
                .collect(),
            type_breakdown: report.analysis.type_breakdown.clone(),
        },
        dedup: FrontendDedupResult {
            groups: report
                .dedup
                .groups
                .iter()
                .take(FRONTEND_DUPLICATE_GROUPS_LIMIT)
                .map(|group| {
                    let mut group = group.clone();
                    if group.files.len() > FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT {
                        group
                            .files
                            .truncate(FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT);
                    }
                    group
                })
                .collect(),
            failures: report.dedup.failures.iter().take(100).cloned().collect(),
            group_count: report.dedup.groups.len(),
            truncated: report.dedup.groups.len() > FRONTEND_DUPLICATE_GROUPS_LIMIT,
        },
        modules: report.modules.clone(),
        advisor: FrontendAdvisorOutput {
            source: report.advisor.source.clone(),
            summary: report.advisor.summary.clone(),
            suggestions: report
                .advisor
                .suggestions
                .iter()
                .take(FRONTEND_SUGGESTIONS_LIMIT)
                .cloned()
                .collect(),
            governance_suggestions: report.advisor.governance_suggestions.clone(),
            suggestion_count: report.advisor.suggestions.len(),
            truncated: report.advisor.suggestions.len() > FRONTEND_SUGGESTIONS_LIMIT,
        },
        failures: report.failures.iter().take(100).cloned().collect(),
        dedup_pending,
        dedup_phase,
        dedup_message,
        dedup_error,
    }
}

fn build_app_overview(report: &ScanReport, limit: usize) -> Vec<AppOverviewItem> {
    let mut buckets: HashMap<String, AppOverviewBucket> = HashMap::new();
    let installed_apps = query_installed_apps().unwrap_or_default();

    for file in &report.scanned_files {
        if let Some(signature) = match_app_signature(&file.path) {
            let bucket =
                buckets
                    .entry(signature.key.to_string())
                    .or_insert_with(|| AppOverviewBucket {
                        key: signature.key.to_string(),
                        identity_key: signature.key.to_string(),
                        app_name: signature.app_name.to_string(),
                        vendor: signature.vendor.to_string(),
                        category: signature.category.to_string(),
                        color: signature.color.to_string(),
                        source_summary: "路径规则".to_string(),
                        status_tags: vec!["路径推断".to_string()],
                        icon_data_uri: None,
                        icon_source: "内置图标".to_string(),
                        detected_root: detect_app_root(&file.path, &signature.components)
                            .unwrap_or_else(|| file.path.to_string_lossy().to_string()),
                        file_count: 0,
                        total_size: 0,
                        sample_paths: BTreeSet::new(),
                    });
            bucket.file_count += 1;
            bucket.total_size += file.size;
            if bucket.sample_paths.len() < 4 {
                bucket
                    .sample_paths
                    .insert(file.path.to_string_lossy().to_string());
            }
        }
    }

    merge_registry_matches(report, &installed_apps, &mut buckets);
    enrich_buckets_with_exe_metadata(&installed_apps, &mut buckets);

    let mut rows = buckets
        .into_values()
        .map(|item| AppOverviewItem {
            icon_data_uri: item
                .icon_data_uri
                .unwrap_or_else(|| build_app_icon_data_uri(&item.app_name, &item.color)),
            key: item.key,
            app_name: item.app_name,
            vendor: item.vendor,
            category: item.category,
            source_summary: item.source_summary,
            status_tags: item.status_tags,
            icon_source: item.icon_source,
            detected_root: item.detected_root,
            file_count: item.file_count,
            total_size: item.total_size,
            sample_paths: item.sample_paths.into_iter().collect(),
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        right
            .total_size
            .cmp(&left.total_size)
            .then_with(|| right.file_count.cmp(&left.file_count))
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    rows.truncate(limit);
    rows
}

fn merge_registry_matches(
    report: &ScanReport,
    installed_apps: &[InstalledAppEntry],
    buckets: &mut HashMap<String, AppOverviewBucket>,
) {
    let mut roots = installed_apps
        .iter()
        .filter_map(|app| {
            app.install_location
                .as_ref()
                .map(|path| (normalize_path_string(path), app))
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| right.0.len().cmp(&left.0.len()));

    for file in &report.scanned_files {
        let file_path = normalize_path_string(&file.path.to_string_lossy());
        for (install_root, app) in &roots {
            if file_path.starts_with(install_root) {
                let bucket = buckets
                    .entry(format!("registry:{}", app.identity_key))
                    .or_insert_with(|| AppOverviewBucket {
                        key: format!("registry:{}", app.identity_key),
                        identity_key: app.identity_key.clone(),
                        app_name: app.display_name.clone(),
                        vendor: app
                            .publisher
                            .clone()
                            .unwrap_or_else(|| "未知厂商".to_string()),
                        category: infer_app_category(&app.display_name),
                        color: infer_color(&app.display_name),
                        source_summary: "注册表卸载项".to_string(),
                        status_tags: vec!["已安装软件".to_string()],
                        icon_data_uri: None,
                        icon_source: "内置图标".to_string(),
                        detected_root: app
                            .install_location
                            .clone()
                            .unwrap_or_else(|| file.path.to_string_lossy().to_string()),
                        file_count: 0,
                        total_size: 0,
                        sample_paths: BTreeSet::new(),
                    });
                bucket.file_count += 1;
                bucket.total_size += file.size;
                if bucket.sample_paths.len() < 4 {
                    bucket
                        .sample_paths
                        .insert(file.path.to_string_lossy().to_string());
                }
                break;
            }
        }
    }

    for bucket in buckets.values_mut() {
        if let Some(app) = installed_apps
            .iter()
            .find(|app| registry_matches_bucket(app, bucket))
        {
            if !bucket.source_summary.contains("注册表") {
                bucket.source_summary.push_str(" + 注册表");
            }
            push_status_tag(bucket, "已安装软件");
            if let Some(publisher) = &app.publisher {
                if bucket.vendor.is_empty() || bucket.vendor == "未知厂商" {
                    bucket.vendor = publisher.clone();
                }
            }
            if let Some(location) = &app.install_location {
                if bucket.detected_root.is_empty() {
                    bucket.detected_root = location.clone();
                }
            }
        }
    }
}

fn enrich_buckets_with_exe_metadata(
    installed_apps: &[InstalledAppEntry],
    buckets: &mut HashMap<String, AppOverviewBucket>,
) {
    for bucket in buckets.values_mut() {
        let Some(app) = installed_apps.iter().find(|app| {
            registry_matches_bucket(app, bucket) || app.identity_key == bucket.identity_key
        }) else {
            continue;
        };

        let Some(candidate_path) = app
            .display_icon
            .as_deref()
            .and_then(parse_display_icon_path)
            .or_else(|| {
                app.install_location
                    .as_deref()
                    .and_then(find_candidate_executable_in_directory)
            })
        else {
            continue;
        };

        let Some(metadata) = read_exe_metadata(&candidate_path) else {
            continue;
        };

        if let Some(product_name) = metadata
            .product_name
            .filter(|value| !value.trim().is_empty())
        {
            bucket.app_name = product_name;
        } else if let Some(file_description) = metadata
            .file_description
            .filter(|value| !value.trim().is_empty())
        {
            bucket.app_name = file_description;
        }
        if let Some(company_name) = metadata
            .company_name
            .filter(|value| !value.trim().is_empty())
        {
            bucket.vendor = company_name;
        }
        if metadata.icon_data_uri.is_some() {
            bucket.icon_data_uri = metadata.icon_data_uri;
            bucket.icon_source = "exe 元数据".to_string();
        }
        if !bucket.source_summary.contains("exe") {
            bucket.source_summary.push_str(" + exe 元数据");
        }
        push_status_tag(bucket, "含 exe 元数据");
    }
}

fn try_restore_cached_report(
    root: &Path,
    config: &AppConfig,
    state: &AppState,
) -> Option<ScanReport> {
    let current_snapshot = query_usn_journal(root)?;
    let cache_path = report_cache_path(state, root);
    let text = fs::read_to_string(cache_path).ok()?;
    let cached: CachedScanReport = serde_json::from_str(&text).ok()?;

    if cached.config_signature != config_signature(config) {
        return None;
    }

    if cached.usn_snapshot != current_snapshot {
        return None;
    }

    Some(cached.report)
}

fn store_cached_report(
    root: &Path,
    config: &AppConfig,
    state: &AppState,
    report: &ScanReport,
) -> anyhow::Result<()> {
    let Some(snapshot) = query_usn_journal(root) else {
        return Ok(());
    };
    let cache_path = report_cache_path(state, root);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = CachedScanReport {
        config_signature: config_signature(config),
        usn_snapshot: snapshot,
        report: report.clone(),
    };
    fs::write(cache_path, serde_json::to_vec(&payload)?)?;
    Ok(())
}

fn load_hash_cache(state: &AppState, root: &Path) -> anyhow::Result<HashCache> {
    let path = hash_cache_path(state, root);
    if !path.exists() {
        return Ok(HashCache::default());
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn load_hash_cache_from_dir(cache_dir: &Path, root: &Path) -> anyhow::Result<HashCache> {
    let path = hash_cache_path_from_dir(cache_dir, root);
    if !path.exists() {
        return Ok(HashCache::default());
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn save_hash_cache(state: &AppState, root: &Path, cache: &HashCache) -> anyhow::Result<()> {
    let path = hash_cache_path(state, root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec(cache)?)?;
    Ok(())
}

fn save_hash_cache_to_dir(cache_dir: &Path, root: &Path, cache: &HashCache) -> anyhow::Result<()> {
    let path = hash_cache_path_from_dir(cache_dir, root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec(cache)?)?;
    Ok(())
}

fn report_cache_path(state: &AppState, root: &Path) -> PathBuf {
    cache_dir(state).join(format!("scan-report-{}.json", root_cache_key(root)))
}

fn report_cache_path_from_dir(cache_dir: &Path, root: &Path) -> PathBuf {
    cache_dir.join(format!("scan-report-{}.json", root_cache_key(root)))
}

fn hash_cache_path(state: &AppState, root: &Path) -> PathBuf {
    cache_dir(state).join(format!("hash-cache-{}.json", root_cache_key(root)))
}

fn hash_cache_path_from_dir(cache_dir: &Path, root: &Path) -> PathBuf {
    cache_dir.join(format!("hash-cache-{}.json", root_cache_key(root)))
}

fn cache_dir(state: &AppState) -> PathBuf {
    state
        .config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cache")
}

fn store_cached_report_to_dir(
    cache_dir: &Path,
    root: &Path,
    config: &AppConfig,
    report: &ScanReport,
) -> anyhow::Result<()> {
    let Some(snapshot) = query_usn_journal(root) else {
        return Ok(());
    };
    let cache_path = report_cache_path_from_dir(cache_dir, root);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = CachedScanReport {
        config_signature: config_signature(config),
        usn_snapshot: snapshot,
        report: report.clone(),
    };
    fs::write(cache_path, serde_json::to_vec(&payload)?)?;
    Ok(())
}

fn root_cache_key(root: &Path) -> String {
    blake3::hash(root.to_string_lossy().as_bytes())
        .to_hex()
        .to_string()
}

fn config_signature(config: &AppConfig) -> String {
    serde_json::to_string(config).unwrap_or_default()
}

fn build_directory_overview(report: &ScanReport, limit: usize) -> Vec<DirectoryOverviewItem> {
    let mut buckets: HashMap<String, DirectoryOverviewBucket> = HashMap::new();

    for file in &report.scanned_files {
        let parts = relative_parts(&file.path, &report.root);
        if parts.is_empty() {
            continue;
        }

        if parts.len() == 1 {
            buckets.insert(
                format!("file:{}", parts[0]),
                DirectoryOverviewBucket {
                    key: format!("file:{}", parts[0]),
                    name: parts[0].clone(),
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    file_count: 1,
                    total_size: file.size,
                    preview: BTreeSet::new(),
                },
            );
            continue;
        }

        let key = format!("dir:{}", parts[0]);
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| DirectoryOverviewBucket {
                key: key.clone(),
                name: parts[0].clone(),
                path: report.root.join(&parts[0]).to_string_lossy().to_string(),
                kind: "directory".to_string(),
                file_count: 0,
                total_size: 0,
                preview: BTreeSet::new(),
            });
        bucket.file_count += 1;
        bucket.total_size += file.size;
        if let Some(next) = parts.get(1) {
            bucket.preview.insert(next.clone());
        }
    }

    for dir_path in &report.analysis.empty_dirs {
        let parts = relative_parts(dir_path, &report.root);
        if parts.is_empty() {
            continue;
        }

        let key = format!("dir:{}", parts[0]);
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| DirectoryOverviewBucket {
                key: key.clone(),
                name: parts[0].clone(),
                path: report.root.join(&parts[0]).to_string_lossy().to_string(),
                kind: "directory".to_string(),
                file_count: 0,
                total_size: 0,
                preview: BTreeSet::new(),
            });
        if let Some(next) = parts.get(1) {
            bucket.preview.insert(next.clone());
        }
    }

    let mut rows = buckets
        .into_values()
        .map(|item| DirectoryOverviewItem {
            key: item.key,
            name: item.name,
            path: item.path,
            kind: item.kind.clone(),
            file_count: item.file_count,
            total_size: item.total_size,
            preview: if item.kind == "file" {
                "-".to_string()
            } else if item.preview.is_empty() {
                "空目录".to_string()
            } else {
                let values: Vec<String> = item.preview.iter().take(4).cloned().collect();
                let mut preview = values.join("、");
                if item.preview.len() > 4 {
                    preview.push_str(" 等");
                }
                preview
            },
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        if left.kind != right.kind {
            return if left.kind == "directory" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if left.total_size != right.total_size {
            return right.total_size.cmp(&left.total_size);
        }
        left.name.cmp(&right.name)
    });
    rows.truncate(limit);
    rows
}

fn build_file_tree_query_result(
    report: &ScanReport,
    query: &str,
    category: &str,
    limit: usize,
) -> FileTreeQueryResult {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() && category == "all" {
        let rows = build_top_level_file_tree_rows(report, limit);
        let matched_count = report.scanned_files.len();
        let node_count = count_nodes(&rows);
        return FileTreeQueryResult {
            matched_count,
            node_count,
            truncated: false,
            rows,
        };
    }

    let mut matched = report
        .scanned_files
        .iter()
        .filter(|file| {
            let ext = file
                .extension
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let full_path = file.path.to_string_lossy().to_ascii_lowercase();
            let query_matched =
                query.is_empty() || full_path.contains(&query) || ext.contains(&query);
            query_matched && matches_file_category(file, category)
        })
        .cloned()
        .collect::<Vec<_>>();

    matched.sort_by(|left, right| left.path.cmp(&right.path));
    let matched_count = matched.len();
    let truncated = matched_count > limit;
    if truncated {
        matched.truncate(limit);
    }

    let rows = build_file_tree_rows(&report.root, &matched);
    let node_count = count_nodes(&rows);

    FileTreeQueryResult {
        matched_count,
        node_count,
        truncated,
        rows,
    }
}

fn build_directory_tree_query_result(
    report: &ScanReport,
    query: &str,
    root_limit: usize,
) -> FileTreeQueryResult {
    let query = query.trim().to_ascii_lowercase();

    #[derive(Clone)]
    struct DirectoryEntry {
      row: FileTreeNode,
      children: Vec<String>,
    }

    let mut node_map: HashMap<String, DirectoryEntry> = HashMap::new();
    let mut roots = Vec::new();

    for file in &report.scanned_files {
        let parts = relative_parts(&file.path, &report.root);
        if parts.is_empty() {
            continue;
        }

        if parts.len() == 1 {
            let key = format!("file:{}", parts[0]);
            node_map.insert(
                key.clone(),
                DirectoryEntry {
                    row: FileTreeNode {
                        key: key.clone(),
                        name: parts[0].clone(),
                        path: file.path.to_string_lossy().to_string(),
                        kind: "file".to_string(),
                        size: file.size,
                        extension: file.extension.clone().unwrap_or_default(),
                        file_count: 1,
                        children: None,
                    },
                    children: Vec::new(),
                },
            );
            roots.push(key);
            continue;
        }

        for depth in 0..parts.len() - 1 {
            let key = format!("dir:{}", parts[..=depth].join("/"));
            if !node_map.contains_key(&key) {
                let path = report.root.join(parts[..=depth].join("/"));
                node_map.insert(
                    key.clone(),
                    DirectoryEntry {
                        row: FileTreeNode {
                            key: key.clone(),
                            name: parts[depth].clone(),
                            path: path.to_string_lossy().to_string(),
                            kind: "directory".to_string(),
                            size: 0,
                            extension: String::new(),
                            file_count: 0,
                            children: None,
                        },
                        children: Vec::new(),
                    },
                );
                if depth == 0 {
                    roots.push(key.clone());
                } else {
                    let parent_key = format!("dir:{}", parts[..depth].join("/"));
                    if let Some(parent) = node_map.get_mut(&parent_key) {
                        if !parent.children.contains(&key) {
                            parent.children.push(key.clone());
                        }
                    }
                }
            }

            if let Some(node) = node_map.get_mut(&key) {
                node.row.size += file.size;
                node.row.file_count += 1;
            }
        }

        let parent_key = format!("dir:{}", parts[..parts.len() - 1].join("/"));
        let file_key = format!("file:{}", parts.join("/"));
        node_map.insert(
            file_key.clone(),
            DirectoryEntry {
                row: FileTreeNode {
                    key: file_key.clone(),
                    name: parts.last().cloned().unwrap_or_default(),
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    size: file.size,
                    extension: file.extension.clone().unwrap_or_default(),
                    file_count: 1,
                    children: None,
                },
                children: Vec::new(),
            },
        );
        if let Some(parent) = node_map.get_mut(&parent_key) {
            if !parent.children.contains(&file_key) {
                parent.children.push(file_key);
            }
        }
    }

    for dir_path in &report.analysis.empty_dirs {
        let parts = relative_parts(dir_path, &report.root);
        if parts.is_empty() {
            continue;
        }

        for depth in 0..parts.len() {
            let key = format!("dir:{}", parts[..=depth].join("/"));
            if !node_map.contains_key(&key) {
                let path = report.root.join(parts[..=depth].join("/"));
                node_map.insert(
                    key.clone(),
                    DirectoryEntry {
                        row: FileTreeNode {
                            key: key.clone(),
                            name: parts[depth].clone(),
                            path: path.to_string_lossy().to_string(),
                            kind: "directory".to_string(),
                            size: 0,
                            extension: String::new(),
                            file_count: 0,
                            children: None,
                        },
                        children: Vec::new(),
                    },
                );
                if depth == 0 {
                    roots.push(key.clone());
                } else {
                    let parent_key = format!("dir:{}", parts[..depth].join("/"));
                    if let Some(parent) = node_map.get_mut(&parent_key) {
                        if !parent.children.contains(&key) {
                            parent.children.push(key.clone());
                        }
                    }
                }
            }
        }
    }

    let matched_roots = roots
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| {
            if query.is_empty() {
                return true;
            }
            node_map
                .get(key)
                .map(|item| item.row.path.to_ascii_lowercase().contains(&query))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    fn build_rows(
        keys: &[String],
        node_map: &HashMap<String, DirectoryEntry>,
    ) -> Vec<FileTreeNode> {
        let mut rows = keys
            .iter()
            .filter_map(|key| node_map.get(key).cloned())
            .collect::<Vec<_>>();

        rows.sort_by(|left, right| {
            right
                .row
                .size
                .cmp(&left.row.size)
                .then_with(|| right.row.file_count.cmp(&left.row.file_count))
                .then_with(|| left.row.name.cmp(&right.row.name))
        });

        rows.into_iter()
            .map(|entry| FileTreeNode {
                children: if entry.children.is_empty() {
                    None
                } else {
                    Some(build_rows(&entry.children, node_map))
                },
                ..entry.row
            })
            .collect()
    }

    let matched_count = matched_roots.len();
    let mut limited_roots = matched_roots;
    if limited_roots.len() > root_limit {
        limited_roots.truncate(root_limit);
    }
    let rows = build_rows(&limited_roots, &node_map);
    let node_count = count_nodes(&rows);

    FileTreeQueryResult {
        matched_count,
        node_count,
        truncated: matched_count > root_limit,
        rows,
    }
}

fn build_top_level_file_tree_rows(report: &ScanReport, limit: usize) -> Vec<FileTreeNode> {
    let mut node_map: HashMap<String, FileTreeNode> = HashMap::new();

    for file in &report.scanned_files {
        let parts = relative_parts(&file.path, &report.root);
        if parts.is_empty() {
            continue;
        }

        let name = parts[0].clone();
        if parts.len() == 1 {
            let key = format!("file:{name}");
            node_map.insert(
                key.clone(),
                FileTreeNode {
                    key,
                    name,
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    size: file.size,
                    extension: file.extension.clone().unwrap_or_default(),
                    file_count: 1,
                    children: None,
                },
            );
            continue;
        }

        let key = format!("dir:{name}");
        let entry = node_map.entry(key.clone()).or_insert_with(|| FileTreeNode {
            key,
            name: name.clone(),
            path: report.root.join(&name).to_string_lossy().to_string(),
            kind: "directory".to_string(),
            size: 0,
            extension: String::new(),
            file_count: 0,
            children: None,
        });
        entry.size += file.size;
        entry.file_count += 1;
    }

    let mut rows = node_map.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        if left.kind != right.kind {
            return if left.kind == "directory" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        right
            .size
            .cmp(&left.size)
            .then_with(|| left.name.cmp(&right.name))
    });
    if rows.len() > limit {
        rows.truncate(limit);
    }
    rows
}

fn build_file_tree_rows(root: &Path, files: &[FileRecord]) -> Vec<FileTreeNode> {
    #[derive(Clone)]
    struct NodeEntry {
        row: FileTreeNode,
        children: Vec<String>,
    }

    let mut node_map: HashMap<String, NodeEntry> = HashMap::new();
    let mut roots = Vec::new();

    for file in files {
        let parts = relative_parts(&file.path, root);
        if parts.is_empty() {
            continue;
        }

        if parts.len() == 1 {
            let key = format!("file:{}", parts[0]);
            node_map.insert(
                key.clone(),
                NodeEntry {
                    row: FileTreeNode {
                        key: key.clone(),
                        name: parts[0].clone(),
                        path: file.path.to_string_lossy().to_string(),
                        kind: "file".to_string(),
                        size: file.size,
                        extension: file.extension.clone().unwrap_or_default(),
                        file_count: 1,
                        children: None,
                    },
                    children: Vec::new(),
                },
            );
            roots.push(key);
            continue;
        }

        for depth in 0..parts.len() - 1 {
            let key = format!("dir:{}", parts[..=depth].join("/"));
            if !node_map.contains_key(&key) {
                node_map.insert(
                    key.clone(),
                    NodeEntry {
                        row: FileTreeNode {
                            key: key.clone(),
                            name: parts[depth].clone(),
                            path: root
                                .join(parts[..=depth].join("/"))
                                .to_string_lossy()
                                .to_string(),
                            kind: "directory".to_string(),
                            size: 0,
                            extension: String::new(),
                            file_count: 0,
                            children: None,
                        },
                        children: Vec::new(),
                    },
                );
                if depth == 0 {
                    roots.push(key.clone());
                } else {
                    let parent_key = format!("dir:{}", parts[..depth].join("/"));
                    if let Some(parent) = node_map.get_mut(&parent_key) {
                        if !parent.children.contains(&key) {
                            parent.children.push(key.clone());
                        }
                    }
                }
            }
            if let Some(node) = node_map.get_mut(&key) {
                node.row.size += file.size;
                node.row.file_count += 1;
            }
        }

        let file_key = format!("file:{}", parts.join("/"));
        node_map.insert(
            file_key.clone(),
            NodeEntry {
                row: FileTreeNode {
                    key: file_key.clone(),
                    name: parts.last().cloned().unwrap_or_default(),
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    size: file.size,
                    extension: file.extension.clone().unwrap_or_default(),
                    file_count: 1,
                    children: None,
                },
                children: Vec::new(),
            },
        );
        let parent_key = format!("dir:{}", parts[..parts.len() - 1].join("/"));
        if let Some(parent) = node_map.get_mut(&parent_key) {
            if !parent.children.contains(&file_key) {
                parent.children.push(file_key);
            }
        }
    }

    fn build_rows(keys: &[String], node_map: &HashMap<String, NodeEntry>) -> Vec<FileTreeNode> {
        let mut rows = keys
            .iter()
            .filter_map(|key| node_map.get(key))
            .cloned()
            .collect::<Vec<_>>();

        rows.sort_by(|left, right| {
            if left.row.kind != right.row.kind {
                return if left.row.kind == "directory" {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            if left.row.size != right.row.size {
                return right.row.size.cmp(&left.row.size);
            }
            left.row.name.cmp(&right.row.name)
        });

        rows.into_iter()
            .map(|entry| FileTreeNode {
                children: if entry.children.is_empty() {
                    None
                } else {
                    Some(build_rows(&entry.children, node_map))
                },
                ..entry.row
            })
            .collect()
    }

    let unique_roots = roots
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    build_rows(&unique_roots, &node_map)
}

fn count_nodes(rows: &[FileTreeNode]) -> usize {
    rows.iter()
        .map(|row| 1 + row.children.as_deref().map(count_nodes).unwrap_or(0))
        .sum()
}

fn relative_parts(path: &Path, root: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn match_app_signature(path: &Path) -> Option<AppSignature> {
    let text = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();

    APP_SIGNATURES
        .iter()
        .find(|signature| {
            signature
                .components
                .iter()
                .all(|needle| text.iter().any(|part| part == needle))
        })
        .copied()
}

fn detect_app_root(path: &Path, markers: &[&str]) -> Option<String> {
    let mut built = PathBuf::new();
    for component in path.components() {
        built.push(component.as_os_str());
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if markers.iter().any(|needle| value == *needle) {
            return Some(built.to_string_lossy().to_string());
        }
    }
    None
}

fn build_app_icon_data_uri(app_name: &str, color: &str) -> String {
    let letter = app_name
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .unwrap_or('A')
        .to_ascii_uppercase();
    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='40' height='40' viewBox='0 0 40 40'><rect width='40' height='40' rx='10' fill='%23{color}'/><text x='20' y='26' text-anchor='middle' font-size='20' font-family='Segoe UI,Arial,sans-serif' fill='white'>{letter}</text></svg>"
    )
}

fn registry_matches_bucket(app: &InstalledAppEntry, bucket: &AppOverviewBucket) -> bool {
    let display = normalize_path_string(&app.display_name);
    let bucket_name = normalize_path_string(&bucket.app_name);
    display.contains(&bucket_name) || bucket_name.contains(&display)
}

fn infer_app_category(name: &str) -> String {
    let text = normalize_path_string(name);
    if text.contains("chrome") || text.contains("edge") || text.contains("firefox") {
        return "浏览器".to_string();
    }
    if text.contains("code")
        || text.contains("idea")
        || text.contains("pycharm")
        || text.contains("goland")
        || text.contains("docker")
    {
        return "开发工具".to_string();
    }
    if text.contains("wechat") || text.contains("qq") || text.contains("telegram") {
        return "社交/通信".to_string();
    }
    if text.contains("office") || text.contains("word") || text.contains("excel") {
        return "办公".to_string();
    }
    "已安装软件".to_string()
}

fn infer_color(name: &str) -> String {
    let palette = [
        "07C160", "1E90FF", "EA4335", "007ACC", "31C27C", "FF3158", "D83B01", "2496ED",
    ];
    let idx = name.bytes().fold(0_usize, |acc, item| acc + item as usize) % palette.len();
    palette[idx].to_string()
}

fn normalize_path_string(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}

fn push_status_tag(bucket: &mut AppOverviewBucket, value: &str) {
    if !bucket.status_tags.iter().any(|tag| tag == value) {
        bucket.status_tags.push(value.to_string());
    }
}

fn parse_display_icon_path(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('"');
    let path = trimmed.split(',').next()?.trim().trim_matches('"');
    if path.is_empty() {
        None
    } else {
        let path_buf = PathBuf::from(path);
        if !is_safe_metadata_executable(&path_buf) {
            return None;
        }
        Some(path.to_string())
    }
}

fn find_candidate_executable_in_directory(dir: &str) -> Option<String> {
    let path = PathBuf::from(dir);
    if !path.is_dir() {
        return None;
    }
    fs::read_dir(path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            is_safe_metadata_executable(path)
        })
        .map(|path| path.to_string_lossy().to_string())
}

fn read_exe_metadata(path: &str) -> Option<ExeMetadata> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        None
    }

    #[cfg(target_os = "windows")]
    {
        let path = Path::new(path);
        if !is_safe_metadata_executable(path) {
            return None;
        }
        read_win32_version_metadata(path)
    }
}

#[cfg(target_os = "windows")]
fn read_win32_version_metadata(path: &Path) -> Option<ExeMetadata> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LangAndCodePage {
        language: u16,
        code_page: u16,
    }

    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn query_string(data: &[u8], sub_block: &str) -> Option<String> {
        let mut buffer = std::ptr::null_mut();
        let mut len = 0u32;
        let sub_block_wide = to_wide(sub_block);
        let ok = VerQueryValueW(
            data.as_ptr() as *const _,
            sub_block_wide.as_ptr(),
            &mut buffer,
            &mut len,
        );
        if ok == 0 || buffer.is_null() || len == 0 {
            return None;
        }
        let slice = std::slice::from_raw_parts(buffer as *const u16, len as usize);
        let end = slice.iter().position(|value| *value == 0).unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..end]).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    unsafe fn query_translation(data: &[u8]) -> Option<LangAndCodePage> {
        let mut buffer = std::ptr::null_mut();
        let mut len = 0u32;
        let sub_block_wide = to_wide(r"\VarFileInfo\Translation");
        let ok = VerQueryValueW(
            data.as_ptr() as *const _,
            sub_block_wide.as_ptr(),
            &mut buffer,
            &mut len,
        );
        if ok == 0 || buffer.is_null() || len < std::mem::size_of::<LangAndCodePage>() as u32 {
            return None;
        }
        Some(*(buffer as *const LangAndCodePage))
    }

    let wide_path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    unsafe {
        let mut handle = 0u32;
        let size = GetFileVersionInfoSizeW(wide_path.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }

        let mut data = vec![0u8; size as usize];
        let ok = GetFileVersionInfoW(
            wide_path.as_ptr(),
            0,
            size,
            data.as_mut_ptr() as *mut _,
        );
        if ok == 0 {
            return None;
        }

        let default_translation = LangAndCodePage {
            language: 0x0409,
            code_page: 1200,
        };
        let translation = query_translation(&data).unwrap_or(default_translation);

        let block_prefix = format!(
            r"\StringFileInfo\{:04x}{:04x}\",
            translation.language, translation.code_page
        );

        let product_name = query_string(&data, &format!("{block_prefix}ProductName"))
            .or_else(|| query_string(&data, r"\StringFileInfo\040904b0\ProductName"));
        let company_name = query_string(&data, &format!("{block_prefix}CompanyName"))
            .or_else(|| query_string(&data, r"\StringFileInfo\040904b0\CompanyName"));
        let file_description = query_string(&data, &format!("{block_prefix}FileDescription"))
            .or_else(|| query_string(&data, r"\StringFileInfo\040904b0\FileDescription"));

        if product_name.is_none() && company_name.is_none() && file_description.is_none() {
            return None;
        }

        Some(ExeMetadata {
            product_name,
            company_name,
            file_description,
            icon_data_uri: None,
        })
    }
}

fn is_safe_metadata_executable(path: &Path) -> bool {
    let is_exe = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    if !is_exe {
        return false;
    }

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    !matches!(
        name.as_str(),
        "uninstall.exe"
            | "uninst.exe"
            | "unins000.exe"
            | "unins001.exe"
            | "setup.exe"
            | "update.exe"
            | "repair.exe"
            | "modify.exe"
            | "maintenancetool.exe"
    ) && !name.contains("uninstall")
        && !name.contains("unins")
        && !name.contains("setup")
        && !name.contains("update")
        && !name.contains("repair")
        && !name.contains("maint")
}

fn query_installed_apps() -> Result<Vec<InstalledAppEntry>, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", INSTALLED_APPS_QUERY_SCRIPT])
            .output()
            .map_err(|error| format!("读取注册表卸载项失败：{error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "注册表卸载项查询失败。".to_string()
            } else {
                format!("注册表卸载项查询失败：{stderr}")
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let entries: Vec<InstalledAppEntry> = serde_json::from_str(trimmed)
            .map_err(|error| format!("解析注册表卸载项失败：{error}"))?;
        Ok(entries)
    }
}

fn matches_file_category(file: &FileRecord, category: &str) -> bool {
    if category == "all" {
        return true;
    }

    let ext = file
        .extension
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext.is_empty() {
        return category == "other";
    }

    let image = matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "svg"
            | "ico"
            | "tif"
            | "tiff"
            | "heic"
            | "heif"
            | "raw"
            | "psd"
            | "avif"
    );
    let video = matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "avi"
            | "mov"
            | "wmv"
            | "flv"
            | "webm"
            | "m4v"
            | "mpeg"
            | "mpg"
            | "ts"
            | "3gp"
            | "rmvb"
    );
    let audio = matches!(
        ext.as_str(),
        "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "m4a"
            | "ogg"
            | "wma"
            | "opus"
            | "ape"
            | "amr"
            | "mid"
            | "midi"
    );
    let archive = matches!(
        ext.as_str(),
        "zip"
            | "zipx"
            | "7z"
            | "rar"
            | "tar"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "cab"
            | "iso"
            | "img"
            | "dmg"
            | "jar"
    );
    let executable = matches!(
        ext.as_str(),
        "exe"
            | "com"
            | "msi"
            | "msix"
            | "msixbundle"
            | "appx"
            | "appxbundle"
            | "bat"
            | "cmd"
            | "ps1"
            | "vbs"
            | "js"
            | "jar"
            | "scr"
    );
    let document = matches!(
        ext.as_str(),
        "pdf"
            | "doc"
            | "docx"
            | "ppt"
            | "pptx"
            | "xls"
            | "xlsx"
            | "csv"
            | "txt"
            | "md"
            | "rtf"
            | "wps"
            | "odt"
            | "ods"
            | "odp"
    );
    let code = matches!(
        ext.as_str(),
        "rs" | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "xml"
            | "ini"
            | "cfg"
            | "conf"
            | "env"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "vue"
            | "py"
            | "java"
            | "kt"
            | "go"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
            | "swift"
            | "scala"
            | "sql"
            | "html"
            | "css"
            | "scss"
            | "less"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "lock"
    );

    match category {
        "image" => image,
        "video" => video,
        "audio" => audio,
        "archive" => archive,
        "executable" => executable,
        "document" => document,
        "code" => code,
        "other" => !(image || video || audio || archive || executable || document || code),
        _ => true,
    }
}

fn query_usn_journal(root: &Path) -> Option<UsnJournalSnapshot> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = root;
        None
    }

    #[cfg(target_os = "windows")]
    {
        let volume = windows_volume_arg(root)?;
        let output = Command::new("cmd")
            .args(["/C", "fsutil", "usn", "queryjournal", &volume])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut journal_id = None;
        let mut next_usn = None;

        for line in stdout.lines() {
            let trimmed = line.trim();
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();
                if key.contains("Journal ID") {
                    journal_id = Some(value);
                } else if key.contains("Next Usn") {
                    next_usn = Some(value);
                }
            }
        }

        Some(UsnJournalSnapshot {
            volume,
            journal_id: journal_id?,
            next_usn: next_usn?,
        })
    }
}

#[cfg(target_os = "windows")]
fn windows_volume_arg(root: &Path) -> Option<String> {
    let text = root.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }
    Some(text[0..2].to_ascii_uppercase())
}

#[derive(Debug, Clone)]
struct DirectoryOverviewBucket {
    key: String,
    name: String,
    path: String,
    kind: String,
    file_count: usize,
    total_size: u64,
    preview: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct AppOverviewBucket {
    key: String,
    identity_key: String,
    app_name: String,
    vendor: String,
    category: String,
    color: String,
    source_summary: String,
    status_tags: Vec<String>,
    icon_data_uri: Option<String>,
    icon_source: String,
    detected_root: String,
    file_count: usize,
    total_size: u64,
    sample_paths: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct AppSignature {
    key: &'static str,
    app_name: &'static str,
    vendor: &'static str,
    category: &'static str,
    color: &'static str,
    components: &'static [&'static str],
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledAppEntry {
    identity_key: String,
    display_name: String,
    publisher: Option<String>,
    install_location: Option<String>,
    display_icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExeMetadata {
    product_name: Option<String>,
    company_name: Option<String>,
    file_description: Option<String>,
    icon_data_uri: Option<String>,
}

const APP_SIGNATURES: &[AppSignature] = &[
    AppSignature {
        key: "wechat",
        app_name: "WeChat",
        vendor: "Tencent",
        category: "社交/通信",
        color: "07C160",
        components: &["wechat files"],
    },
    AppSignature {
        key: "qq",
        app_name: "QQ",
        vendor: "Tencent",
        category: "社交/通信",
        color: "1E90FF",
        components: &["tencent", "qq"],
    },
    AppSignature {
        key: "qqmusic",
        app_name: "QQ Music",
        vendor: "Tencent",
        category: "音频/娱乐",
        color: "31C27C",
        components: &["qqmusic"],
    },
    AppSignature {
        key: "wechat_devtools",
        app_name: "WeChat DevTools",
        vendor: "Tencent",
        category: "开发工具",
        color: "19A974",
        components: &["wechatdevtools"],
    },
    AppSignature {
        key: "chrome",
        app_name: "Google Chrome",
        vendor: "Google",
        category: "浏览器",
        color: "EA4335",
        components: &["google", "chrome"],
    },
    AppSignature {
        key: "edge",
        app_name: "Microsoft Edge",
        vendor: "Microsoft",
        category: "浏览器",
        color: "0A84FF",
        components: &["microsoft", "edge"],
    },
    AppSignature {
        key: "vscode",
        app_name: "VS Code",
        vendor: "Microsoft",
        category: "开发工具",
        color: "007ACC",
        components: &["code"],
    },
    AppSignature {
        key: "pycharm",
        app_name: "PyCharm",
        vendor: "JetBrains",
        category: "开发工具",
        color: "21D789",
        components: &["pycharm"],
    },
    AppSignature {
        key: "idea",
        app_name: "IntelliJ IDEA",
        vendor: "JetBrains",
        category: "开发工具",
        color: "FF3158",
        components: &["intellijidea"],
    },
    AppSignature {
        key: "goland",
        app_name: "GoLand",
        vendor: "JetBrains",
        category: "开发工具",
        color: "00C4B3",
        components: &["goland"],
    },
    AppSignature {
        key: "python",
        app_name: "Python",
        vendor: "Python Software Foundation",
        category: "开发环境",
        color: "3776AB",
        components: &["python"],
    },
    AppSignature {
        key: "conda",
        app_name: "Conda",
        vendor: "Anaconda",
        category: "开发环境",
        color: "44A833",
        components: &["anaconda3"],
    },
    AppSignature {
        key: "conda-mini",
        app_name: "Miniconda",
        vendor: "Anaconda",
        category: "开发环境",
        color: "4CB944",
        components: &["miniconda3"],
    },
    AppSignature {
        key: "nodejs",
        app_name: "Node.js",
        vendor: "OpenJS Foundation",
        category: "开发环境",
        color: "539E43",
        components: &["nodejs"],
    },
    AppSignature {
        key: "docker",
        app_name: "Docker",
        vendor: "Docker",
        category: "开发工具",
        color: "2496ED",
        components: &["docker"],
    },
    AppSignature {
        key: "steam",
        app_name: "Steam",
        vendor: "Valve",
        category: "游戏平台",
        color: "171A21",
        components: &["steam"],
    },
    AppSignature {
        key: "obsidian",
        app_name: "Obsidian",
        vendor: "Obsidian",
        category: "知识管理",
        color: "483699",
        components: &["obsidian"],
    },
    AppSignature {
        key: "adobe",
        app_name: "Adobe",
        vendor: "Adobe",
        category: "创意设计",
        color: "FF0000",
        components: &["adobe"],
    },
    AppSignature {
        key: "office",
        app_name: "Microsoft Office",
        vendor: "Microsoft",
        category: "办公",
        color: "D83B01",
        components: &["microsoft office"],
    },
];

#[cfg(target_os = "windows")]
const INSTALLED_APPS_QUERY_SCRIPT: &str = r#"
$paths = @(
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
)

$rows = foreach ($path in $paths) {
  Get-ItemProperty -Path $path -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName } | ForEach-Object {
    [pscustomobject]@{
      identityKey = if ($_.PSChildName) { [string]$_.PSChildName } else { [string]$_.DisplayName }
      displayName = [string]$_.DisplayName
      publisher = if ($_.Publisher) { [string]$_.Publisher } else { $null }
      installLocation = if ($_.InstallLocation) { [string]$_.InstallLocation } else { $null }
      displayIcon = if ($_.DisplayIcon) { [string]$_.DisplayIcon } else { $null }
    }
  }
}

@($rows) | ConvertTo-Json -Depth 4 -Compress
"#;
