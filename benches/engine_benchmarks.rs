use chrono::Utc;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use simplebash_pos_backend::domain::reports::{CategorySalesRow, PatternCell, TimeSeriesPoint};
use simplebash_pos_backend::modules::reports::engine::charts::{
    analyze_sales_patterns, enrich_timeseries, top_categories_with_other,
};

/// Generates mock continuous time-series data for benchmarking.
fn generate_mock_timeseries(num_points: usize) -> Vec<TimeSeriesPoint> {
    let now = Utc::now();
    (0..num_points)
        .map(|i| TimeSeriesPoint {
            period_start: now,
            label: format!("Day {i}"),
            revenue_cents: 100_000 + ((i * 137) % 50_000) as i64,
            retail_revenue_cents: 70_000 + ((i * 89) % 30_000) as i64,
            repair_revenue_cents: 20_000 + ((i * 41) % 15_000) as i64,
            print_revenue_cents: 10_000 + ((i * 17) % 5_000) as i64,
            discount_cents: 5_000,
            cogs_cents: 50_000,
            gross_profit_cents: 45_000,
            gross_margin_bps: 4500,
            commission_cents: 5_000,
            net_profit_cents: 40_000,
            invoice_count: 15,
        })
        .collect()
}

/// Generates mock category sales rows.
fn generate_mock_category_rows(count: usize) -> Vec<CategorySalesRow> {
    (0..count)
        .map(|i| CategorySalesRow {
            category_key: Some(format!("cat_{i}")),
            category_name: format!("Product Category {i}"),
            subcategory_key: None,
            subcategory_name: None,
            units_sold: (count - i) as i64 * 10,
            revenue_cents: (count - i) as i64 * 50_000,
            discount_cents: 1_000,
            cogs_cents: (count - i) as i64 * 30_000,
            gross_profit_cents: (count - i) as i64 * 19_000,
            gross_margin_bps: 3800,
        })
        .collect()
}

/// Generates 168-cell sales pattern heatmap (7 days x 24 hours).
fn generate_mock_pattern_cells() -> Vec<PatternCell> {
    let mut cells = Vec::with_capacity(168);
    for weekday in 1..=7 {
        for hour in 0..24 {
            let rev = if (9..=19).contains(&hour) {
                150_000 + ((weekday as i64 * 37 + hour as i64 * 73) % 80_000)
            } else {
                5_000
            };
            cells.push(PatternCell {
                weekday,
                hour,
                invoice_count: if rev > 10_000 { 12 } else { 1 },
                revenue_cents: rev,
            });
        }
    }
    cells
}

fn bench_enrich_timeseries(c: &mut Criterion) {
    let mut group = c.benchmark_group("reports_timeseries_enrichment");

    for size in [30, 90, 365].iter() {
        let points = generate_mock_timeseries(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_s| {
            b.iter(|| {
                let enriched = enrich_timeseries(black_box(&points), black_box(7));
                assert_eq!(enriched.len(), *size);
            });
        });
    }

    group.finish();
}

fn bench_category_rollups(c: &mut Criterion) {
    let mut group = c.benchmark_group("reports_category_rollups");

    for size in [10, 50, 200].iter() {
        let rows = generate_mock_category_rows(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_s| {
            b.iter(|| {
                let rolled = top_categories_with_other(black_box(rows.clone()), black_box(5));
                assert!(rolled.len() <= 6);
            });
        });
    }

    group.finish();
}

fn bench_sales_patterns(c: &mut Criterion) {
    let cells = generate_mock_pattern_cells();
    c.bench_function("reports_analyze_sales_patterns_168_cells", |b| {
        b.iter(|| {
            let (peak, max_rev) = analyze_sales_patterns(black_box(&cells));
            assert!(peak.is_some());
            assert!(max_rev > 0);
        });
    });
}

fn bench_backup_record_serialization(c: &mut Criterion) {
    use serde_json::json;

    let mock_rows: Vec<_> = (0..1000)
        .map(|i| {
            json!({
                "id": format!("inv_{i}"),
                "invoice_number": format!("INV-{:06}", i),
                "total_cents": 145000,
                "customer_id": "cust_12345",
                "cashier_id": "usr_999",
                "items_count": 4,
                "created_at": "2026-09-13T12:00:00Z"
            })
        })
        .collect();

    c.bench_function("backup_serialize_1000_rows_json", |b| {
        b.iter(|| {
            let serialized = serde_json::to_string(black_box(&mock_rows)).unwrap();
            black_box(serialized);
        });
    });
}

criterion_group!(
    benches,
    bench_enrich_timeseries,
    bench_category_rollups,
    bench_sales_patterns,
    bench_backup_record_serialization
);
criterion_main!(benches);
