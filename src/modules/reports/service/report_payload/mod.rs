// Builds the `serde_json::Value` payload for the document-server
// `analytics-report` Typst template (`GET /reports/analytics/document`).
// Mirrors `billing::service::print_payload` in spirit: it does no rendering,
// only shaping — every series/table arrives pre-aggregated from the same
// `service::analytics` functions the JSON endpoints use, flattened into the
// parallel-array contract the `.typ` reads from `sys.inputs`.
//
// The shop letterhead (`shopName` / `shopAddressLines` / `logoUrl`) is *not*
// sent from here — it is stored once in the document-server's `template-data`
// for the `analytics-report` template and deep-merged in at render time.

mod build_analytics_report_data;
mod money;
mod pct;
#[cfg(test)]
mod tests;

pub(crate) use build_analytics_report_data::build_analytics_report_data;
