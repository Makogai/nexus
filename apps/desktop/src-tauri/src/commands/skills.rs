//! Skills page (ADR-0046): every skill a session can get, and the person's editor for workspace and project skills.
//! Writes happen only here, on the person's action — never through the MCP server.

use tauri::State;

use nexus_core::skills::{self, Saved, Scope, SkillEntry, SkillFile, SkillRef};

use crate::error::{IpcResult, blocking};
use crate::state::Ctx;

fn conn(ctx: &Ctx) -> IpcResult<nexus_core::db::Connection> {
    Ok(nexus_core::db::open(&ctx.data_dir.join(nexus_core::db::DB_FILE))?)
}

fn ws(c: &nexus_core::db::Connection, id: &str) -> IpcResult<nexus_core::workspace::Workspace> {
    Ok(nexus_core::workspace::get(c, id)?)
}

#[tauri::command]
pub async fn skills_list(ctx: State<'_, Ctx>, workspace_id: String) -> IpcResult<Vec<SkillEntry>> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(skills::list(&c, &ctx.data_dir, &ws(&c, &workspace_id)?)?)
    })
    .await
}

#[tauri::command]
pub async fn skills_read(ctx: State<'_, Ctx>, workspace_id: String, skill: SkillRef) -> IpcResult<SkillFile> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(skills::read(&c, &ctx.data_dir, &ws(&c, &workspace_id)?, &skill)?)
    })
    .await
}

#[tauri::command]
pub async fn skills_create(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    scope: Scope,
    project_id: Option<String>,
    name: String,
    description: String,
) -> IpcResult<Saved> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(skills::create(&c, &ctx.data_dir, &ws(&c, &workspace_id)?, scope, project_id.as_deref(), &name, &description)?)
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn skills_save(
    ctx: State<'_, Ctx>,
    workspace_id: String,
    scope: Scope,
    project_id: Option<String>,
    name: String,
    text: String,
    expected_hash: String,
) -> IpcResult<Saved> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        Ok(skills::save(&c, &ctx.data_dir, &ws(&c, &workspace_id)?, scope, project_id.as_deref(), &name, &text, &expected_hash)?)
    })
    .await
}

#[tauri::command]
pub async fn skills_delete(ctx: State<'_, Ctx>, workspace_id: String, scope: Scope, project_id: Option<String>, name: String, expected_hash: String) -> IpcResult<()> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let c = conn(&ctx)?;
        skills::delete(&c, &ctx.data_dir, &ws(&c, &workspace_id)?, scope, project_id.as_deref(), &name, &expected_hash)?;
        Ok(())
    })
    .await
}
