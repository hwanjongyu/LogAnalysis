use super::bookmark::Bookmark;
use crate::filter::heatmap::LineSeverity;

/// Configuration options for generating an interactive standalone HTML incident report.
#[derive(Clone, Debug)]
pub struct HtmlReportOptions {
    pub file_name: String,
    pub total_lines: usize,
    pub bookmarks: Vec<Bookmark>,
}

/// Generates a 100% self-contained, interactive HTML incident dossier with zero external dependencies.
pub fn generate_html_report(options: &HtmlReportOptions) -> String {
    let now = chrono_timestamp_approx();
    let escaped_filename = escape_html(&options.file_name);
    let total_bookmarks = options.bookmarks.len();

    let mut error_count = 0;
    let mut warn_count = 0;
    let mut info_count = 0;

    for b in &options.bookmarks {
        match b.severity {
            LineSeverity::Error => error_count += 1,
            LineSeverity::Warning => warn_count += 1,
            _ => info_count += 1,
        }
    }

    let mut rows_html = String::new();
    for b in &options.bookmarks {
        let (sev_class, sev_label, sev_badge_class) = match b.severity {
            LineSeverity::Error => ("error", "ERROR", "badge-error"),
            LineSeverity::Warning => ("warning", "WARN", "badge-warn"),
            LineSeverity::RuleMatch => ("match", "MATCH", "badge-match"),
            LineSeverity::Normal => ("info", "INFO", "badge-info"),
        };

        let note_html = if let Some(ref note) = b.note {
            format!(
                r#"<div class="bookmark-note"><span class="note-icon">📝</span> {}</div>"#,
                escape_html(note)
            )
        } else {
            String::new()
        };

        let escaped_text = escape_html(&b.text);

        rows_html.push_str(&format!(
            r#"<tr class="log-row {sev_class}" data-severity="{sev_class}">
    <td class="col-line">{line_num}</td>
    <td class="col-sev"><span class="badge {sev_badge_class}">{sev_label}</span></td>
    <td class="col-text">
        <div class="log-text">{escaped_text}</div>
        {note_html}
    </td>
    <td class="col-action">
        <button class="btn-copy" onclick="copyRow(this)" title="Copy log line">📋 Copy</button>
    </td>
</tr>
"#,
            line_num = b.line_number,
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>LogAnalysis Incident Report - {escaped_filename}</title>
<style>
:root {{
    --bg-page: #f8fafc;
    --bg-card: #ffffff;
    --bg-header: #0f172a;
    --border: #e2e8f0;
    --text-primary: #0f172a;
    --text-muted: #64748b;
    --text-code: #1e293b;
    --accent: #2563eb;
    --row-hover: #f1f5f9;
    --err-bg: #fef2f2;
    --err-text: #dc2626;
    --warn-bg: #fffbeb;
    --warn-text: #d97706;
    --info-bg: #eff6ff;
    --info-text: #2563eb;
}}
.dark {{
    --bg-page: #0b0f19;
    --bg-card: #111827;
    --bg-header: #030712;
    --border: #1f2937;
    --text-primary: #f9fafb;
    --text-muted: #9ca3af;
    --text-code: #e5e7eb;
    --accent: #3b82f6;
    --row-hover: #1e293b;
    --err-bg: #450a0a33;
    --err-text: #ef4444;
    --warn-bg: #451a0333;
    --warn-text: #f59e0b;
    --info-bg: #17255433;
    --info-text: #60a5fa;
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    background: var(--bg-page);
    color: var(--text-primary);
    line-height: 1.5;
    padding-bottom: 60px;
    transition: background 0.15s ease, color 0.15s ease;
}}
.header {{
    background: var(--bg-header);
    color: #ffffff;
    padding: 24px 32px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border);
}}
.header h1 {{
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 8px;
}}
.header-actions {{
    display: flex;
    align-items: center;
    gap: 12px;
}}
.btn-theme {{
    background: rgba(255, 255, 255, 0.1);
    color: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    transition: background 0.2s;
}}
.btn-theme:hover {{ background: rgba(255, 255, 255, 0.2); }}
.container {{
    max-width: 1400px;
    margin: 24px auto;
    padding: 0 24px;
}}
.summary-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
    margin-bottom: 24px;
}}
.summary-card {{
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.05);
}}
.summary-card .label {{ font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--text-muted); }}
.summary-card .value {{ font-size: 24px; font-weight: 700; margin-top: 4px; }}
.summary-card.err .value {{ color: var(--err-text); }}
.summary-card.warn .value {{ color: var(--warn-text); }}
.summary-card.info .value {{ color: var(--info-text); }}
.summary-card.total .value {{ color: var(--accent); }}

.filter-bar {{
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
    margin-bottom: 16px;
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
}}
.search-input {{
    flex: 1;
    min-width: 250px;
    padding: 8px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-page);
    color: var(--text-primary);
    font-size: 14px;
    outline: none;
}}
.search-input:focus {{ border-color: var(--accent); }}
.pill-group {{ display: flex; gap: 6px; }}
.pill-btn {{
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-page);
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
}}
.pill-btn.active {{
    background: var(--accent);
    color: #ffffff;
    border-color: var(--accent);
}}

.table-wrap {{
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0,0,0,0.05);
}}
table {{
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
}}
th {{
    background: var(--bg-page);
    text-align: left;
    padding: 10px 14px;
    font-weight: 600;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
}}
td {{
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
}}
tr:last-child td {{ border-bottom: none; }}
tr:hover td {{ background: var(--row-hover); }}
.col-line {{
    width: 80px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-weight: 600;
    color: var(--text-muted);
}}
.col-sev {{ width: 90px; }}
.col-text {{
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: var(--text-code);
    word-break: break-all;
}}
.col-action {{ width: 80px; text-align: right; }}
.badge {{
    display: inline-block;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 700;
    font-family: ui-monospace, SFMono-Regular, monospace;
}}
.badge-error {{ background: var(--err-bg); color: var(--err-text); }}
.badge-warn {{ background: var(--warn-bg); color: var(--warn-text); }}
.badge-info {{ background: var(--info-bg); color: var(--info-text); }}
.badge-match {{ background: var(--info-bg); color: var(--info-text); }}
.btn-copy {{
    background: transparent;
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-muted);
}}
.btn-copy:hover {{ background: var(--border); color: var(--text-primary); }}
.bookmark-note {{
    margin-top: 6px;
    font-family: -apple-system, sans-serif;
    font-size: 12px;
    color: var(--accent);
    background: var(--info-bg);
    padding: 4px 8px;
    border-radius: 4px;
    display: inline-block;
}}
.footer-info {{
    margin-top: 16px;
    text-align: center;
    font-size: 12px;
    color: var(--text-muted);
}}
</style>
</head>
<body>
<div class="header">
    <h1>🚀 LogAnalysis Incident Dossier</h1>
    <div class="header-actions">
        <button class="btn-theme" onclick="toggleTheme()">🌓 Dark / Light</button>
    </div>
</div>

<div class="container">
    <div class="summary-grid">
        <div class="summary-card">
            <div class="label">Log Source File</div>
            <div class="value" style="font-size: 16px; word-break: break-all;">{escaped_filename}</div>
        </div>
        <div class="summary-card total">
            <div class="label">Bookmarked Key Lines</div>
            <div class="value">{total_bookmarks}</div>
        </div>
        <div class="summary-card err">
            <div class="label">Fatal / Errors</div>
            <div class="value">{error_count}</div>
        </div>
        <div class="summary-card warn">
            <div class="label">Warnings</div>
            <div class="value">{warn_count}</div>
        </div>
        <div class="summary-card info">
            <div class="label">Info / Normal</div>
            <div class="value">{info_count}</div>
        </div>
    </div>

    <div class="filter-bar">
        <input type="text" id="filterInput" class="search-input" placeholder="Live search across bookmarked lines and notes..." oninput="applyFilters()">
        <div class="pill-group">
            <button class="pill-btn active" onclick="setSeverityFilter('all', this)">ALL</button>
            <button class="pill-btn" onclick="setSeverityFilter('error', this)">ERRORS</button>
            <button class="pill-btn" onclick="setSeverityFilter('warning', this)">WARNINGS</button>
            <button class="pill-btn" onclick="setSeverityFilter('info', this)">INFO</button>
        </div>
    </div>

    <div class="table-wrap">
        <table id="logTable">
            <thead>
                <tr>
                    <th class="col-line">Line</th>
                    <th class="col-sev">Severity</th>
                    <th>Log Statement & Notes</th>
                    <th class="col-action">Action</th>
                </tr>
            </thead>
            <tbody>
                {rows_html}
            </tbody>
        </table>
    </div>

    <div class="footer-info">
        Generated by <strong>LogAnalysis</strong> (GPU-Accelerated Rust + GPUI) • {now}
    </div>
</div>

<script>
let currentSeverity = 'all';

function toggleTheme() {{
    document.documentElement.classList.toggle('dark');
}}

function setSeverityFilter(sev, btn) {{
    currentSeverity = sev;
    document.querySelectorAll('.pill-btn').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
    applyFilters();
}}

function applyFilters() {{
    const query = document.getElementById('filterInput').value.toLowerCase();
    const rows = document.querySelectorAll('#logTable tbody tr');

    rows.forEach(row => {{
        const text = row.querySelector('.col-text').textContent.toLowerCase();
        const rowSev = row.getAttribute('data-severity');

        const matchesQuery = !query || text.includes(query);
        const matchesSev = currentSeverity === 'all' || rowSev === currentSeverity;

        row.style.display = (matchesQuery && matchesSev) ? '' : 'none';
    }});
}}

function copyRow(btn) {{
    const text = btn.closest('tr').querySelector('.log-text').textContent;
    navigator.clipboard.writeText(text).then(() => {{
        const orig = btn.textContent;
        btn.textContent = '✓ Copied';
        setTimeout(() => {{ btn.textContent = orig; }}, 1500);
    }});
}}
</script>
</body>
</html>
"#
    )
}

/// Escapes standard HTML special characters to prevent injection/broken layouts.
#[inline]
pub fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(c),
        }
    }
    output
}

fn chrono_timestamp_approx() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("Epoch {}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_report_xss_escaping() {
        let malicious = "<script>alert('pwned')</script>&\"'";
        let escaped = escape_html(malicious);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(escaped.contains("&lt;script&gt;"));
        assert!(escaped.contains("&amp;&quot;&#39;"));
    }

    #[test]
    fn test_html_report_generation_offline() {
        let options = HtmlReportOptions {
            file_name: "test.log".into(),
            total_lines: 1000,
            bookmarks: vec![
                Bookmark {
                    source_idx: 42,
                    line_number: 43,
                    text: "11-06 18:32:00 E Tag: Fatal crash trace".into(),
                    note: Some("Crash root cause".into()),
                    severity: LineSeverity::Error,
                    created_at_epoch: 1730935949,
                },
            ],
        };

        let html = generate_html_report(&options);
        // Verify self-contained offline nature
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(html.contains("Fatal crash trace"));
        assert!(html.contains("Crash root cause"));
        assert!(html.contains("test.log"));
    }
}
