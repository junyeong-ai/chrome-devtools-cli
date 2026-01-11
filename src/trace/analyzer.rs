use crate::chrome::models::{
    CoreWebVitals, MainThreadMetrics, PageLoadMetrics, PerformanceAnalysis, PerformanceTrace,
    Rating, Recommendation, Severity, TraceEvent,
};

pub fn analyze_trace(trace: &PerformanceTrace, url: String) -> PerformanceAnalysis {
    let nav_start = find_navigation_start(&trace.events);
    let core_web_vitals = calculate_core_web_vitals(&trace.events, nav_start);
    let page_load_metrics = calculate_page_load_metrics(&trace.events, nav_start);
    let main_thread_metrics = calculate_main_thread_metrics(&trace.events);
    let recommendations = generate_recommendations(&core_web_vitals, &main_thread_metrics);

    PerformanceAnalysis {
        url,
        core_web_vitals,
        page_load_metrics,
        main_thread_metrics,
        recommendations,
    }
}

fn find_navigation_start(events: &[TraceEvent]) -> f64 {
    events
        .iter()
        .find(|e| e.name == "navigationStart")
        .map(|e| e.timestamp)
        .unwrap_or_else(|| {
            events
                .iter()
                .map(|e| e.timestamp)
                .filter(|&ts| ts > 0.0 && ts.is_finite())
                .min_by(|a, b| a.total_cmp(b))
                .unwrap_or(0.0)
        })
}

pub fn calculate_core_web_vitals(events: &[TraceEvent], nav_start: f64) -> CoreWebVitals {
    let lcp_ms = calculate_lcp(events, nav_start);
    let fid_ms = calculate_fid(events);
    let cls = calculate_cls(events);
    let ttfb_ms = calculate_ttfb(events);

    CoreWebVitals {
        lcp_ms,
        fid_ms,
        cls,
        ttfb_ms,
        lcp_rating: rate_lcp(lcp_ms),
        fid_rating: rate_fid(fid_ms),
        cls_rating: rate_cls(cls),
        ttfb_rating: rate_ttfb(ttfb_ms),
    }
}

fn calculate_lcp(events: &[TraceEvent], nav_start: f64) -> Option<f64> {
    events
        .iter()
        .filter(|e| e.name == "largestContentfulPaint::Candidate")
        .filter_map(|e| {
            e.args
                .as_ref()
                .and_then(|args| args.get("data"))
                .and_then(|data| data.get("size"))
                .and_then(|s| s.as_f64())
                .filter(|v| v.is_finite())
        })
        .max_by(|a, b| a.total_cmp(b))
        .map(|_| {
            events
                .iter()
                .rfind(|e| e.name == "largestContentfulPaint::Candidate")
                .map(|e| (e.timestamp - nav_start) / 1000.0)
                .unwrap_or(0.0)
        })
}

fn calculate_fid(events: &[TraceEvent]) -> Option<f64> {
    events
        .iter()
        .find(|e| e.name == "firstInputDelay")
        .and_then(|e| {
            e.args
                .as_ref()
                .and_then(|args| args.get("data"))
                .and_then(|data| data.get("delay"))
                .and_then(|d| d.as_f64())
        })
}

fn calculate_cls(events: &[TraceEvent]) -> Option<f64> {
    let shift_sum: f64 = events
        .iter()
        .filter(|e| e.name == "LayoutShift")
        .filter_map(|e| {
            e.args
                .as_ref()
                .and_then(|args| args.get("data"))
                .and_then(|data| data.get("score"))
                .and_then(|s| s.as_f64())
        })
        .sum();

    if shift_sum > 0.0 {
        Some(shift_sum)
    } else {
        None
    }
}

fn calculate_ttfb(events: &[TraceEvent]) -> Option<f64> {
    events
        .iter()
        .find(|e| e.name == "ResourceReceiveResponse" && e.args.is_some())
        .and_then(|e| {
            let ts = e.timestamp;
            events
                .iter()
                .find(|req| req.name == "ResourceSendRequest")
                .map(|req| (ts - req.timestamp) / 1000.0)
        })
}

fn rate_lcp(lcp_ms: Option<f64>) -> Rating {
    match lcp_ms {
        Some(ms) if ms < 2500.0 => Rating::Good,
        Some(ms) if ms < 4000.0 => Rating::NeedsImprovement,
        Some(_) => Rating::Poor,
        None => Rating::Good,
    }
}

fn rate_fid(fid_ms: Option<f64>) -> Rating {
    match fid_ms {
        Some(ms) if ms < 100.0 => Rating::Good,
        Some(ms) if ms < 300.0 => Rating::NeedsImprovement,
        Some(_) => Rating::Poor,
        None => Rating::Good,
    }
}

fn rate_cls(cls: Option<f64>) -> Rating {
    match cls {
        Some(score) if score < 0.1 => Rating::Good,
        Some(score) if score < 0.25 => Rating::NeedsImprovement,
        Some(_) => Rating::Poor,
        None => Rating::Good,
    }
}

fn rate_ttfb(ttfb_ms: Option<f64>) -> Rating {
    match ttfb_ms {
        Some(ms) if ms < 800.0 => Rating::Good,
        Some(ms) if ms < 1800.0 => Rating::NeedsImprovement,
        Some(_) => Rating::Poor,
        None => Rating::Good,
    }
}

fn calculate_page_load_metrics(events: &[TraceEvent], nav_start: f64) -> PageLoadMetrics {
    let dom_content_loaded_ms = events
        .iter()
        .find(|e| e.name == "domContentLoadedEventEnd")
        .map(|e| (e.timestamp - nav_start) / 1000.0)
        .unwrap_or(0.0);

    let load_complete_ms = events
        .iter()
        .find(|e| e.name == "loadEventEnd")
        .map(|e| (e.timestamp - nav_start) / 1000.0)
        .unwrap_or(0.0);

    let first_paint_ms = events
        .iter()
        .find(|e| e.name == "firstPaint")
        .map(|e| (e.timestamp - nav_start) / 1000.0);

    let first_contentful_paint_ms = events
        .iter()
        .find(|e| e.name == "firstContentfulPaint")
        .map(|e| (e.timestamp - nav_start) / 1000.0);

    PageLoadMetrics {
        dom_content_loaded_ms,
        load_complete_ms,
        first_paint_ms,
        first_contentful_paint_ms,
    }
}

fn calculate_main_thread_metrics(events: &[TraceEvent]) -> MainThreadMetrics {
    let long_tasks: Vec<&TraceEvent> = events
        .iter()
        .filter(|e| e.name == "RunTask" && e.dur.unwrap_or(0.0) > 50000.0)
        .collect();

    let total_blocking_time_ms = long_tasks
        .iter()
        .filter_map(|e| e.dur)
        .map(|dur| (dur / 1000.0 - 50.0).max(0.0))
        .sum();

    let script_duration_ms = events
        .iter()
        .filter(|e| e.category.contains("devtools.timeline") && e.name == "EvaluateScript")
        .filter_map(|e| e.dur)
        .sum::<f64>()
        / 1000.0;

    MainThreadMetrics {
        total_blocking_time_ms,
        long_tasks_count: long_tasks.len(),
        script_duration_ms,
    }
}

pub fn generate_recommendations(
    vitals: &CoreWebVitals,
    main_thread: &MainThreadMetrics,
) -> Vec<Recommendation> {
    let mut recommendations = Vec::new();

    if let Some(ttfb) = vitals.ttfb_ms
        && ttfb > 800.0
    {
        recommendations.push(Recommendation {
                category: "Server Response".to_string(),
                severity: if ttfb > 1800.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                message: format!(
                    "Time to First Byte is {}ms. Consider server-side optimizations, CDN usage, or caching.",
                    ttfb as u64
                ),
                metric_value: Some(ttfb),
            });
    }

    if let Some(lcp) = vitals.lcp_ms
        && lcp > 2500.0
    {
        recommendations.push(Recommendation {
                category: "Largest Contentful Paint".to_string(),
                severity: if lcp > 4000.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                message: format!(
                    "LCP is {}ms. Optimize largest content element, use image optimization, and preload critical resources.",
                    lcp as u64
                ),
                metric_value: Some(lcp),
            });
    }

    if let Some(fid) = vitals.fid_ms
        && fid > 100.0
    {
        recommendations.push(Recommendation {
            category: "First Input Delay".to_string(),
            severity: if fid > 300.0 {
                Severity::High
            } else {
                Severity::Medium
            },
            message: format!(
                "FID is {}ms. Reduce JavaScript execution time and break up long tasks.",
                fid as u64
            ),
            metric_value: Some(fid),
        });
    }

    if let Some(cls) = vitals.cls
        && cls > 0.1
    {
        recommendations.push(Recommendation {
                category: "Cumulative Layout Shift".to_string(),
                severity: if cls > 0.25 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                message: format!(
                    "CLS is {:.3}. Add size attributes to images/videos and avoid inserting content above existing content.",
                    cls
                ),
                metric_value: Some(cls),
            });
    }

    if main_thread.long_tasks_count > 3 {
        recommendations.push(Recommendation {
            category: "Main Thread".to_string(),
            severity: Severity::Medium,
            message: format!(
                "{} long tasks detected. Break up long-running JavaScript into smaller chunks.",
                main_thread.long_tasks_count
            ),
            metric_value: Some(main_thread.long_tasks_count as f64),
        });
    }

    if main_thread.total_blocking_time_ms > 300.0 {
        recommendations.push(Recommendation {
            category: "Total Blocking Time".to_string(),
            severity: if main_thread.total_blocking_time_ms > 600.0 {
                Severity::High
            } else {
                Severity::Medium
            },
            message: format!(
                "Total Blocking Time is {}ms. Reduce JavaScript execution time.",
                main_thread.total_blocking_time_ms as u64
            ),
            metric_value: Some(main_thread.total_blocking_time_ms),
        });
    }

    recommendations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_lcp() {
        assert_eq!(rate_lcp(Some(2000.0)), Rating::Good);
        assert_eq!(rate_lcp(Some(3000.0)), Rating::NeedsImprovement);
        assert_eq!(rate_lcp(Some(5000.0)), Rating::Poor);
        assert_eq!(rate_lcp(None), Rating::Good);
    }

    #[test]
    fn test_rate_fid() {
        assert_eq!(rate_fid(Some(50.0)), Rating::Good);
        assert_eq!(rate_fid(Some(200.0)), Rating::NeedsImprovement);
        assert_eq!(rate_fid(Some(400.0)), Rating::Poor);
    }

    #[test]
    fn test_rate_cls() {
        assert_eq!(rate_cls(Some(0.05)), Rating::Good);
        assert_eq!(rate_cls(Some(0.15)), Rating::NeedsImprovement);
        assert_eq!(rate_cls(Some(0.3)), Rating::Poor);
    }
}
