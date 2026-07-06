use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Initialize the database. Connects to `~/.fission/sessions.db` by default.
    pub fn init() -> Result<Self> {
        let db_path = Self::db_path()?;
        
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let conn = Connection::open(&db_path)?;
        Self::run_migrations(&conn)?;
        Ok(Self { conn })
    }

    fn db_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home.join(".fission").join("sessions.db"))
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )",
            [],
        )?;
        
        Ok(())
    }

    pub fn insert_session(&self, title: &str, provider: &str, model: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (title, provider, model) VALUES (?1, ?2, ?3)",
            params![title, provider, model],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_session_title(&self, session_id: i64, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![title, session_id],
        )?;
        Ok(())
    }

    pub fn insert_message(&self, session_id: i64, role: &str, content: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO messages (session_id, role, content) VALUES (?1, ?2, ?3)",
            params![session_id, role, content],
        )?;
        self.conn.execute(
            "UPDATE sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![session_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, provider, model, created_at, updated_at FROM sessions ORDER BY updated_at DESC"
        )?;
        
        let session_iter = stmt.query_map([], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                title: row.get(1)?,
                provider: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        
        let mut sessions = Vec::new();
        for s in session_iter {
            sessions.push(s?);
        }
        Ok(sessions)
    }

    pub fn get_messages(&self, session_id: i64) -> Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC"
        )?;
        
        let msg_iter = stmt.query_map(params![session_id], |row| {
            Ok(MessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        
        let mut messages = Vec::new();
        for m in msg_iter {
            messages.push(m?);
        }
        Ok(messages)
    }

    pub fn clear_messages(&self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }
}
