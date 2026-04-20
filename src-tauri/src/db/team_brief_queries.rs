use crate::models::TeamBrief;
use rusqlite::params;
use tracing::debug;

use super::Database;

impl Database {
    /// Upsert the brief for a team. PRIMARY KEY (project_id, team) collision
    /// → update content + tokens + timestamp in place. Member snapshot and
    /// tokens are stored for audit.
    pub fn upsert_team_brief(
        &self,
        project_id: &str,
        team: &str,
        content: &str,
        member_piece_ids: &[String],
        tokens_used: i64,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let members_json = serde_json::to_string(member_piece_ids).map_err(|e| e.to_string())?;
        debug!(project_id, team, tokens_used, "Upserting team brief");

        self.conn
            .execute(
                "INSERT INTO team_briefs (team, project_id, content, member_piece_ids_json, tokens_used, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(project_id, team) DO UPDATE SET \
                    content = excluded.content, \
                    member_piece_ids_json = excluded.member_piece_ids_json, \
                    tokens_used = excluded.tokens_used, \
                    updated_at = excluded.updated_at",
                params![team, project_id, content, members_json, tokens_used, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_team_brief(
        &self,
        project_id: &str,
        team: &str,
    ) -> Result<Option<TeamBrief>, String> {
        let result = self.conn.query_row(
            "SELECT team, project_id, content, member_piece_ids_json, tokens_used, updated_at \
             FROM team_briefs WHERE project_id = ?1 AND team = ?2",
            params![project_id, team],
            |row| {
                let members_json: String = row.get(3)?;
                Ok(TeamBrief {
                    team: row.get(0)?,
                    project_id: row.get(1)?,
                    content: row.get(2)?,
                    member_piece_ids: serde_json::from_str(&members_json).unwrap_or_default(),
                    tokens_used: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(brief) => Ok(Some(brief)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// List all briefs for a project, newest first. When `exclude_team` is
    /// provided (e.g. the caller piece's own team), rows for that team are
    /// filtered out — the expected shape at prompt-build time.
    pub fn list_team_briefs_for_project(
        &self,
        project_id: &str,
        exclude_team: Option<&str>,
    ) -> Result<Vec<TeamBrief>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT team, project_id, content, member_piece_ids_json, tokens_used, updated_at \
                 FROM team_briefs WHERE project_id = ?1 \
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![project_id], |row| {
                let members_json: String = row.get(3)?;
                Ok(TeamBrief {
                    team: row.get(0)?,
                    project_id: row.get(1)?,
                    content: row.get(2)?,
                    member_piece_ids: serde_json::from_str(&members_json).unwrap_or_default(),
                    tokens_used: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut briefs: Vec<TeamBrief> = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        if let Some(skip) = exclude_team {
            briefs.retain(|brief| brief.team != skip);
        }
        Ok(briefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_db(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "project-builder-team-briefs-{case}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir.join("data.db")
    }

    fn cleanup(db_path: &Path) {
        if let Some(parent) = db_path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn upsert_then_get_returns_latest_content() {
        let db_path = temp_db("upsert");
        let db = Database::new_at_path(&db_path).expect("open db");
        let project = db
            .create_project("Brief test", "seed")
            .expect("create project");

        db.upsert_team_brief(
            &project.id,
            "payments",
            "initial",
            &["p1".to_string()],
            100,
        )
        .expect("first upsert");
        let first = db
            .get_team_brief(&project.id, "payments")
            .expect("get")
            .expect("brief exists");
        assert_eq!(first.content, "initial");

        db.upsert_team_brief(
            &project.id,
            "payments",
            "updated",
            &["p1".to_string(), "p2".to_string()],
            200,
        )
        .expect("second upsert");
        let second = db
            .get_team_brief(&project.id, "payments")
            .expect("get")
            .expect("brief exists");
        assert_eq!(second.content, "updated");
        assert_eq!(second.member_piece_ids, vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(second.tokens_used, 200);

        // The PRIMARY KEY collision means there's still exactly one row.
        let all = db
            .list_team_briefs_for_project(&project.id, None)
            .expect("list");
        assert_eq!(all.len(), 1);

        cleanup(&db_path);
    }

    #[test]
    fn list_team_briefs_excludes_caller_team() {
        let db_path = temp_db("exclude");
        let db = Database::new_at_path(&db_path).expect("open db");
        let project = db.create_project("Exclude", "seed").expect("create");

        db.upsert_team_brief(&project.id, "auth", "a", &[], 10).expect("upsert auth");
        db.upsert_team_brief(&project.id, "payments", "p", &[], 10).expect("upsert payments");
        db.upsert_team_brief(&project.id, "ingest", "i", &[], 10).expect("upsert ingest");

        let without_payments = db
            .list_team_briefs_for_project(&project.id, Some("payments"))
            .expect("list");
        let teams: Vec<_> = without_payments.iter().map(|b| b.team.as_str()).collect();
        assert_eq!(teams.len(), 2);
        assert!(teams.contains(&"auth"));
        assert!(teams.contains(&"ingest"));
        assert!(!teams.contains(&"payments"));

        cleanup(&db_path);
    }

    #[test]
    fn list_team_briefs_orders_by_updated_desc() {
        let db_path = temp_db("order");
        let db = Database::new_at_path(&db_path).expect("open db");
        let project = db.create_project("Order", "seed").expect("create");

        db.upsert_team_brief(&project.id, "a", "one", &[], 0).expect("a");
        // Force a distinguishable timestamp; SQLite's default resolution may
        // collide on same-second writes.
        std::thread::sleep(std::time::Duration::from_millis(50));
        db.upsert_team_brief(&project.id, "b", "two", &[], 0).expect("b");
        std::thread::sleep(std::time::Duration::from_millis(50));
        db.upsert_team_brief(&project.id, "c", "three", &[], 0).expect("c");

        let listed = db
            .list_team_briefs_for_project(&project.id, None)
            .expect("list");
        assert_eq!(listed[0].team, "c");
        assert_eq!(listed[1].team, "b");
        assert_eq!(listed[2].team, "a");

        cleanup(&db_path);
    }

    #[test]
    fn get_team_brief_returns_none_for_missing() {
        let db_path = temp_db("missing");
        let db = Database::new_at_path(&db_path).expect("open db");
        let project = db.create_project("Missing", "seed").expect("create");

        let got = db
            .get_team_brief(&project.id, "nonexistent")
            .expect("query");
        assert!(got.is_none());

        cleanup(&db_path);
    }
}
