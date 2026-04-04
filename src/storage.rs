use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::models::{
    AdminTokenRecord, LinkInspectionRecord, SubmissionMode, SubmissionRecord, SubmissionStatus,
    TeacherChallengeRecord, TeacherTokenRecord,
};

#[derive(Clone, Debug)]
pub struct RetrievalEventRecord {
    pub submission_id: String,
    pub studnum: String,
    pub retrieved_at: String,
    pub scheduled_delete_at: Option<String>,
}

pub struct Store {
    connection: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn new(config: &AppConfig) -> AppResult<Self> {
        if let Some(parent) = config.db_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::internal(format!("创建数据库目录失败: {error}")))?;
        }

        let connection = Connection::open(&config.db_path)
            .map_err(|error| AppError::internal(format!("打开 SQLite 失败: {error}")))?;

        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(|error| AppError::internal(format!("初始化 SQLite 参数失败: {error}")))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn init_schema(&self) -> AppResult<()> {
        let sql = r#"
        CREATE TABLE IF NOT EXISTS submissions (
            submission_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            studnum TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_sha256 TEXT NOT NULL,
            accepted_at TEXT NOT NULL,
            mode TEXT NOT NULL,
            payload_kind TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            status TEXT NOT NULL,
            server_sha256 TEXT NOT NULL,
            retrieved_at TEXT,
            scheduled_delete_at TEXT
        );

        CREATE TABLE IF NOT EXISTS link_inspections (
            submission_id TEXT PRIMARY KEY,
            has_git_dir INTEGER NOT NULL,
            zip_entries_summary TEXT NOT NULL,
            duplicate_sha256 TEXT,
            duplicate_submission_ids TEXT NOT NULL,
            inspected_at TEXT NOT NULL,
            FOREIGN KEY(submission_id) REFERENCES submissions(submission_id)
        );

        CREATE TABLE IF NOT EXISTS teacher_challenges (
            challenge_id TEXT PRIMARY KEY,
            public_key_fingerprint TEXT NOT NULL,
            challenge_b64 TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            used INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS teacher_tokens (
            token TEXT PRIMARY KEY,
            issued_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            bound_public_key_fingerprint TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS admin_tokens (
            token TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            issued_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS retrieval_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            submission_id TEXT NOT NULL,
            studnum TEXT NOT NULL,
            retrieved_at TEXT NOT NULL,
            scheduled_delete_at TEXT
        );
        "#;

        self.connection()?
            .execute_batch(sql)
            .map_err(|error| AppError::internal(format!("创建数据表失败: {error}")))?;

        Ok(())
    }

    pub fn insert_submission(&self, record: &SubmissionRecord) -> AppResult<()> {
        self.connection()?
            .execute(
                r#"
                INSERT INTO submissions (
                    submission_id, name, studnum, file_name, file_sha256, accepted_at,
                    mode, payload_kind, storage_path, status, server_sha256,
                    retrieved_at, scheduled_delete_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    record.submission_id,
                    record.name,
                    record.studnum,
                    record.file_name,
                    record.file_sha256,
                    record.accepted_at.to_rfc3339(),
                    record.mode.as_str(),
                    record.payload_kind.as_str(),
                    record.storage_path,
                    record.status.as_str(),
                    record.server_sha256,
                    record
                        .retrieved_at
                        .as_ref()
                        .map(DateTime::<Utc>::to_rfc3339),
                    record
                        .scheduled_delete_at
                        .as_ref()
                        .map(DateTime::<Utc>::to_rfc3339),
                ],
            )
            .map_err(map_sqlite_error("保存提交记录失败"))?;

        Ok(())
    }

    pub fn insert_link_inspection(&self, inspection: &LinkInspectionRecord) -> AppResult<()> {
        let entries = serde_json::to_string(&inspection.zip_entries_summary)
            .map_err(|error| AppError::internal(format!("序列化 ZIP 摘要失败: {error}")))?;
        let duplicates = serde_json::to_string(&inspection.duplicate_submission_ids)
            .map_err(|error| AppError::internal(format!("序列化重复列表失败: {error}")))?;

        self.connection()?
            .execute(
                r#"
                INSERT OR REPLACE INTO link_inspections (
                    submission_id, has_git_dir, zip_entries_summary,
                    duplicate_sha256, duplicate_submission_ids, inspected_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    inspection.submission_id,
                    inspection.has_git_dir,
                    entries,
                    inspection.duplicate_sha256,
                    duplicates,
                    inspection.inspected_at,
                ],
            )
            .map_err(map_sqlite_error("保存审查结果失败"))?;

        Ok(())
    }

    pub fn update_submission_status(
        &self,
        submission_id: &str,
        status: SubmissionStatus,
    ) -> AppResult<()> {
        self.connection()?
            .execute(
                "UPDATE submissions SET status = ?1 WHERE submission_id = ?2",
                params![status.as_str(), submission_id],
            )
            .map_err(map_sqlite_error("更新提交状态失败"))?;

        Ok(())
    }

    pub fn find_duplicate_submission_ids(
        &self,
        server_sha256: &str,
        current_submission_id: &str,
    ) -> AppResult<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT submission_id
                FROM submissions
                WHERE server_sha256 = ?1
                  AND submission_id != ?2
                  AND status != 'deleted'
                ORDER BY accepted_at DESC
                "#,
            )
            .map_err(map_sqlite_error("查询重复提交失败"))?;

        let rows = statement
            .query_map(params![server_sha256, current_submission_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_sqlite_error("读取重复提交失败"))?;

        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(map_sqlite_error("读取重复提交失败"))?);
        }

        Ok(ids)
    }

    pub fn insert_teacher_challenge(&self, record: &TeacherChallengeRecord) -> AppResult<()> {
        self.connection()?
            .execute(
                r#"
                INSERT INTO teacher_challenges (
                    challenge_id, public_key_fingerprint, challenge_b64,
                    created_at, expires_at, used
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    record.challenge_id,
                    record.public_key_fingerprint,
                    record.challenge_b64,
                    record.created_at,
                    record.expires_at,
                    record.used,
                ],
            )
            .map_err(map_sqlite_error("保存教师挑战失败"))?;

        Ok(())
    }

    pub fn get_teacher_challenge(
        &self,
        challenge_id: &str,
    ) -> AppResult<Option<TeacherChallengeRecord>> {
        self.connection()?
            .query_row(
                r#"
                SELECT challenge_id, public_key_fingerprint, challenge_b64, created_at, expires_at, used
                FROM teacher_challenges
                WHERE challenge_id = ?1
                "#,
                params![challenge_id],
                |row| {
                    Ok(TeacherChallengeRecord {
                        challenge_id: row.get(0)?,
                        public_key_fingerprint: row.get(1)?,
                        challenge_b64: row.get(2)?,
                        created_at: row.get(3)?,
                        expires_at: row.get(4)?,
                        used: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error("读取教师挑战失败"))
    }

    pub fn mark_teacher_challenge_used(&self, challenge_id: &str) -> AppResult<()> {
        self.connection()?
            .execute(
                "UPDATE teacher_challenges SET used = 1 WHERE challenge_id = ?1",
                params![challenge_id],
            )
            .map_err(map_sqlite_error("更新挑战状态失败"))?;

        Ok(())
    }

    pub fn insert_teacher_token(&self, record: &TeacherTokenRecord) -> AppResult<()> {
        self.connection()?
            .execute(
                r#"
                INSERT INTO teacher_tokens (token, issued_at, expires_at, bound_public_key_fingerprint)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    record.token,
                    record.issued_at,
                    record.expires_at,
                    record.bound_public_key_fingerprint,
                ],
            )
            .map_err(map_sqlite_error("保存访问令牌失败"))?;

        Ok(())
    }

    pub fn get_teacher_token(&self, token: &str) -> AppResult<Option<TeacherTokenRecord>> {
        self.connection()?
            .query_row(
                r#"
                SELECT token, issued_at, expires_at, bound_public_key_fingerprint
                FROM teacher_tokens
                WHERE token = ?1
                "#,
                params![token],
                |row| {
                    Ok(TeacherTokenRecord {
                        token: row.get(0)?,
                        issued_at: row.get(1)?,
                        expires_at: row.get(2)?,
                        bound_public_key_fingerprint: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error("读取访问令牌失败"))
    }

    pub fn insert_admin_token(&self, record: &AdminTokenRecord) -> AppResult<()> {
        self.connection()?
            .execute(
                r#"
                INSERT INTO admin_tokens (token, username, issued_at, expires_at)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    record.token,
                    record.username,
                    record.issued_at,
                    record.expires_at,
                ],
            )
            .map_err(map_sqlite_error("保存管理员访问令牌失败"))?;

        Ok(())
    }

    pub fn get_admin_token(&self, token: &str) -> AppResult<Option<AdminTokenRecord>> {
        self.connection()?
            .query_row(
                r#"
                SELECT token, username, issued_at, expires_at
                FROM admin_tokens
                WHERE token = ?1
                "#,
                params![token],
                |row| {
                    Ok(AdminTokenRecord {
                        token: row.get(0)?,
                        username: row.get(1)?,
                        issued_at: row.get(2)?,
                        expires_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error("读取管理员访问令牌失败"))
    }

    pub fn list_submissions(&self, studnum: Option<&str>) -> AppResult<Vec<SubmissionRecord>> {
        let sql = if studnum.is_some() {
            r#"
            SELECT submission_id, name, studnum, file_name, file_sha256, accepted_at,
                   mode, payload_kind, storage_path, status, server_sha256,
                   retrieved_at, scheduled_delete_at
            FROM submissions
            WHERE studnum = ?1 AND status != 'deleted'
            ORDER BY accepted_at DESC
            "#
        } else {
            r#"
            SELECT submission_id, name, studnum, file_name, file_sha256, accepted_at,
                   mode, payload_kind, storage_path, status, server_sha256,
                   retrieved_at, scheduled_delete_at
            FROM submissions
            WHERE status != 'deleted'
            ORDER BY accepted_at DESC
            "#
        };

        let connection = self.connection()?;
        let mut statement = connection
            .prepare(sql)
            .map_err(map_sqlite_error("查询提交列表失败"))?;

        let rows = if let Some(studnum) = studnum {
            statement.query_map(params![studnum], map_submission_row)
        } else {
            statement.query_map([], map_submission_row)
        }
        .map_err(map_sqlite_error("读取提交列表失败"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(map_sqlite_error("读取提交列表失败"))?);
        }

        Ok(items)
    }

    pub fn get_submission_by_id(&self, submission_id: &str) -> AppResult<Option<SubmissionRecord>> {
        self.connection()?
            .query_row(
                r#"
                SELECT submission_id, name, studnum, file_name, file_sha256, accepted_at,
                       mode, payload_kind, storage_path, status, server_sha256,
                       retrieved_at, scheduled_delete_at
                FROM submissions
                WHERE submission_id = ?1
                "#,
                params![submission_id],
                map_submission_row,
            )
            .optional()
            .map_err(map_sqlite_error("读取提交详情失败"))
    }

    pub fn get_link_inspection(
        &self,
        submission_id: &str,
    ) -> AppResult<Option<LinkInspectionRecord>> {
        self.connection()?
            .query_row(
                r#"
                SELECT submission_id, has_git_dir, zip_entries_summary,
                       duplicate_sha256, duplicate_submission_ids, inspected_at
                FROM link_inspections
                WHERE submission_id = ?1
                "#,
                params![submission_id],
                |row| {
                    let entries: String = row.get(2)?;
                    let duplicates: String = row.get(4)?;
                    Ok(LinkInspectionRecord {
                        submission_id: row.get(0)?,
                        has_git_dir: row.get(1)?,
                        zip_entries_summary: serde_json::from_str(&entries).unwrap_or_default(),
                        duplicate_sha256: row.get(3)?,
                        duplicate_submission_ids: serde_json::from_str(&duplicates)
                            .unwrap_or_default(),
                        inspected_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error("读取审查详情失败"))
    }

    pub fn schedule_submission_deletion(
        &self,
        submission_ids: &[String],
        retrieved_at: &str,
        scheduled_delete_at: &str,
    ) -> AppResult<()> {
        let connection = self.connection()?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(map_sqlite_error("创建删除计划事务失败"))?;

        for submission_id in submission_ids {
            let studnum: String = transaction
                .query_row(
                    "SELECT studnum FROM submissions WHERE submission_id = ?1",
                    params![submission_id],
                    |row| row.get(0),
                )
                .map_err(map_sqlite_error("读取学号失败"))?;

            transaction
                .execute(
                    r#"
                    UPDATE submissions
                    SET status = ?1, retrieved_at = ?2, scheduled_delete_at = ?3
                    WHERE submission_id = ?4
                    "#,
                    params![
                        SubmissionStatus::ScheduledDelete.as_str(),
                        retrieved_at,
                        scheduled_delete_at,
                        submission_id,
                    ],
                )
                .map_err(map_sqlite_error("写入删除计划失败"))?;

            transaction
                .execute(
                    r#"
                    INSERT INTO retrieval_events (submission_id, studnum, retrieved_at, scheduled_delete_at)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                    params![submission_id, studnum, retrieved_at, scheduled_delete_at],
                )
                .map_err(map_sqlite_error("记录取件事件失败"))?;
        }

        transaction
            .commit()
            .map_err(map_sqlite_error("提交删除计划事务失败"))?;

        Ok(())
    }

    pub fn list_expired_submission_paths(
        &self,
        now_rfc3339: &str,
    ) -> AppResult<Vec<(String, String)>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT submission_id, storage_path
                FROM submissions
                WHERE scheduled_delete_at IS NOT NULL
                  AND scheduled_delete_at <= ?1
                  AND status != 'deleted'
                ORDER BY scheduled_delete_at ASC
                "#,
            )
            .map_err(map_sqlite_error("查询过期删除任务失败"))?;

        let rows = statement
            .query_map(params![now_rfc3339], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(map_sqlite_error("读取过期删除任务失败"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(map_sqlite_error("读取过期删除任务失败"))?);
        }

        Ok(items)
    }

    pub fn mark_submission_deleted(&self, submission_id: &str) -> AppResult<()> {
        self.connection()?
            .execute(
                "UPDATE submissions SET status = 'deleted' WHERE submission_id = ?1",
                params![submission_id],
            )
            .map_err(map_sqlite_error("标记提交删除失败"))?;

        Ok(())
    }

    pub fn list_recent_challenges(&self, limit: usize) -> AppResult<Vec<TeacherChallengeRecord>> {
        let limit = i64::try_from(limit).map_err(|_| AppError::internal("limit 超出范围"))?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT challenge_id, public_key_fingerprint, challenge_b64, created_at, expires_at, used
                FROM teacher_challenges
                ORDER BY created_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(map_sqlite_error("查询挑战记录失败"))?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(TeacherChallengeRecord {
                    challenge_id: row.get(0)?,
                    public_key_fingerprint: row.get(1)?,
                    challenge_b64: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                    used: row.get(5)?,
                })
            })
            .map_err(map_sqlite_error("读取挑战记录失败"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(map_sqlite_error("读取挑战记录失败"))?);
        }

        Ok(items)
    }

    pub fn list_recent_tokens(&self, limit: usize) -> AppResult<Vec<TeacherTokenRecord>> {
        let limit = i64::try_from(limit).map_err(|_| AppError::internal("limit 超出范围"))?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT token, issued_at, expires_at, bound_public_key_fingerprint
                FROM teacher_tokens
                ORDER BY issued_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(map_sqlite_error("查询令牌记录失败"))?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(TeacherTokenRecord {
                    token: row.get(0)?,
                    issued_at: row.get(1)?,
                    expires_at: row.get(2)?,
                    bound_public_key_fingerprint: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error("读取令牌记录失败"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(map_sqlite_error("读取令牌记录失败"))?);
        }

        Ok(items)
    }

    pub fn list_recent_retrievals(&self, limit: usize) -> AppResult<Vec<RetrievalEventRecord>> {
        let limit = i64::try_from(limit).map_err(|_| AppError::internal("limit 超出范围"))?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT submission_id, studnum, retrieved_at, scheduled_delete_at
                FROM retrieval_events
                ORDER BY retrieved_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(map_sqlite_error("查询取件记录失败"))?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(RetrievalEventRecord {
                    submission_id: row.get(0)?,
                    studnum: row.get(1)?,
                    retrieved_at: row.get(2)?,
                    scheduled_delete_at: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error("读取取件记录失败"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(map_sqlite_error("读取取件记录失败"))?);
        }

        Ok(items)
    }

    fn connection(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AppError::internal("数据库锁已损坏"))
    }
}

fn map_submission_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubmissionRecord> {
    Ok(SubmissionRecord {
        submission_id: row.get(0)?,
        name: row.get(1)?,
        studnum: row.get(2)?,
        file_name: row.get(3)?,
        file_sha256: row.get(4)?,
        accepted_at: parse_datetime(row.get::<_, String>(5)?),
        mode: SubmissionMode::from_db(&row.get::<_, String>(6)?).unwrap_or(SubmissionMode::Link),
        payload_kind: SubmissionMode::from_db(&row.get::<_, String>(7)?)
            .unwrap_or(SubmissionMode::Link),
        storage_path: row.get(8)?,
        status: SubmissionStatus::from_db(&row.get::<_, String>(9)?)
            .unwrap_or(SubmissionStatus::Accepted),
        server_sha256: row.get(10)?,
        retrieved_at: row.get::<_, Option<String>>(11)?.map(parse_datetime),
        scheduled_delete_at: row.get::<_, Option<String>>(12)?.map(parse_datetime),
    })
}

fn parse_datetime(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn map_sqlite_error(prefix: &'static str) -> impl Fn(rusqlite::Error) -> AppError {
    move |error| AppError::internal(format!("{prefix}: {error}"))
}

pub fn remove_file_if_exists(path: &Path) -> AppResult<()> {
    match fs::remove_file(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::internal(format!("删除文件失败: {error}"))),
    }
}
