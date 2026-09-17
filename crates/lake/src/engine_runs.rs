//! What each CLI invoke cost, when the CLI said so.
//!
//! One row per child process, not per document. A batch of files is one
//! invoke and one bill. Rows without a number are not written: inventing
//! `$0` for a silent CLI is the same defect as inventing a duration.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use rusqlite::params;

use crate::{Lake, Result};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct NewEngineRun {
    pub engine: String,
    pub stage: String,
    pub subject: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EngineRun {
    pub id: String,
    pub at: String,
    pub engine: String,
    pub stage: String,
    pub subject: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
}

/// Totals for one engine today, summing only the columns that were filled.
#[derive(Debug, Clone)]
pub struct EngineSpend {
    pub engine: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
}

impl Lake {
    pub fn record_engine_run(&self, run: NewEngineRun) -> Result<String> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("run:{unique}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let at = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO engine_runs
                (id, at, engine, stage, subject, input_tokens, output_tokens, cache_tokens, cost_usd, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                at,
                run.engine,
                run.stage,
                run.subject,
                run.input_tokens,
                run.output_tokens,
                run.cache_tokens,
                run.cost_usd,
                run.model,
            ],
        )?;
        Ok(id)
    }

    pub fn recent_engine_runs(&self, limit: usize) -> Result<Vec<EngineRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, at, engine, stage, subject, input_tokens, output_tokens, cache_tokens, cost_usd, model
             FROM engine_runs
             ORDER BY at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_run)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn last_engine_run(&self) -> Result<Option<EngineRun>> {
        Ok(self.recent_engine_runs(1)?.into_iter().next())
    }

    /// Per-engine totals for local today. `SUM` skips NULL, so a run that
    /// reported tokens but no price does not become `$0` in the total.
    pub fn engine_spend_today(&self) -> Result<Vec<EngineSpend>> {
        let start = local_today_start();
        let mut stmt = self.conn.prepare(
            "SELECT engine,
                    SUM(input_tokens),
                    SUM(output_tokens),
                    SUM(cache_tokens),
                    SUM(cost_usd)
             FROM engine_runs
             WHERE at >= ?1
             GROUP BY engine
             ORDER BY engine",
        )?;
        let rows = stmt.query_map(params![start], |row| {
            Ok(EngineSpend {
                engine: row.get(0)?,
                input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                cache_tokens: row.get(3)?,
                cost_usd: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn local_today_start() -> String {
    use chrono::TimeZone;
    let today = chrono::Local::now().date_naive();
    chrono::Local
        .from_local_datetime(&today.and_time(chrono::NaiveTime::MIN))
        .single()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| {
            Utc::now()
                .date_naive()
                .and_time(chrono::NaiveTime::MIN)
                .and_utc()
                .to_rfc3339()
        })
}

fn row_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<EngineRun> {
    Ok(EngineRun {
        id: row.get(0)?,
        at: row.get(1)?,
        engine: row.get(2)?,
        stage: row.get(3)?,
        subject: row.get(4)?,
        input_tokens: row.get(5)?,
        output_tokens: row.get(6)?,
        cache_tokens: row.get(7)?,
        cost_usd: row.get(8)?,
        model: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lake;

    #[test]
    fn a_run_the_cli_did_not_price_is_stored_without_a_dollar_figure() {
        let lake = Lake::in_memory().unwrap();
        lake.record_engine_run(NewEngineRun {
            engine: "Codex".into(),
            stage: "candidates".into(),
            subject: "Cjenik.xlsx".into(),
            input_tokens: Some(4000),
            output_tokens: Some(218),
            cache_tokens: None,
            cost_usd: None,
            model: Some("gpt-5".into()),
        })
        .unwrap();

        let run = &lake.recent_engine_runs(10).unwrap()[0];
        assert_eq!(run.engine, "Codex");
        assert_eq!(run.subject, "Cjenik.xlsx");
        assert_eq!(run.input_tokens, Some(4000));
        assert_eq!(run.output_tokens, Some(218));
        assert!(run.cost_usd.is_none(), "silence is not $0");
    }

    #[test]
    fn today_spend_sums_only_rows_that_have_numbers() {
        let lake = Lake::in_memory().unwrap();
        lake.record_engine_run(priced("Codex", 100, 10, Some(0.02)))
            .unwrap();
        lake.record_engine_run(priced("Codex", 50, 5, None)).unwrap();
        lake.record_engine_run(priced("Claude Code", 20, 2, Some(0.01)))
            .unwrap();

        let today = lake.engine_spend_today().unwrap();
        let codex = today.iter().find(|row| row.engine == "Codex").unwrap();
        assert_eq!(codex.input_tokens, Some(150));
        assert_eq!(codex.output_tokens, Some(15));
        assert_eq!(codex.cost_usd, Some(0.02), "a silent price is skipped, not zeroed");

        let claude = today.iter().find(|row| row.engine == "Claude Code").unwrap();
        assert_eq!(claude.cost_usd, Some(0.01));
    }

    fn priced(engine: &str, input: i64, output: i64, cost: Option<f64>) -> NewEngineRun {
        NewEngineRun {
            engine: engine.into(),
            stage: "candidates".into(),
            subject: "Cjenik.xlsx".into(),
            input_tokens: Some(input),
            output_tokens: Some(output),
            cache_tokens: None,
            cost_usd: cost,
            model: None,
        }
    }
}
