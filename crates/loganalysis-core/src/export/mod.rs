pub mod bookmark;
pub mod html_report;

pub use bookmark::{Bookmark, BookmarkManager};
pub use html_report::{escape_html, generate_html_report, HtmlReportOptions};
