use crate::runtime::{
    extract_backend_mode, extract_command_args, extract_command_name,
    resolve_project_root_from_envelope, with_project_root_cwd,
};
use crate::spec_bridge::{
    handle_archive, handle_check, handle_complete, handle_handoff, handle_list, handle_next,
    handle_packet, handle_prompt, handle_show,
};
use crate::work_commands::{
    handle_work_check, handle_work_complete, handle_work_handoff, handle_work_list,
    handle_work_next, handle_work_packet, handle_work_prompt, handle_work_show,
};
use crate::work_fields::handle_schema;
use std::path::PathBuf;

pub(crate) fn dispatch_data_from_envelope(
    envelope: &serde_json::Value,
) -> Result<(String, String, PathBuf, serde_json::Value), String> {
    let command =
        extract_command_name(envelope).ok_or_else(|| "missing command payload".to_string())?;
    let backend_mode = extract_backend_mode(envelope);
    let args = extract_command_args(envelope);
    let project_root = resolve_project_root_from_envelope(envelope)?;

    let data =
        with_project_root_cwd(&project_root, || match command.as_str() {
            "list" => {
                let work = handle_work_list(&project_root, args)?;
                if work.as_array().is_some_and(|items| items.is_empty()) {
                    handle_list(&project_root, args)
                } else {
                    Ok(work)
                }
            }
            "next" => {
                let work = handle_work_next(&project_root, args)?;
                if work.as_array().is_some_and(|items| items.is_empty()) {
                    handle_next(&project_root)
                } else {
                    Ok(work)
                }
            }
            "check" => handle_work_check(&project_root, args)
                .or_else(|_| handle_check(&project_root, args)),
            "show" => {
                handle_work_show(&project_root, args).or_else(|_| handle_show(&project_root, args))
            }
            "prompt" => handle_work_prompt(&project_root, args)
                .or_else(|_| handle_prompt(&project_root, args)),
            "handoff" => handle_work_handoff(&project_root, args)
                .or_else(|_| handle_handoff(&project_root, args)),
            "packet" => handle_work_packet(&project_root, args)
                .or_else(|_| handle_packet(&project_root, args)),
            "complete" => handle_work_complete(&project_root, args)
                .or_else(|_| handle_complete(&project_root, args)),
            "archive" => handle_archive(&project_root, args),
            "schema" => Ok(handle_schema()),
            _ => Ok(serde_json::json!({
                "status": "scaffold",
                "message": format!("command '{}' not implemented", command),
                "command": command,
            })),
        })?;

    Ok((command, backend_mode, project_root, data))
}

#[cfg(test)]
pub(crate) fn dispatch_for_test(command_json: &str) -> Result<serde_json::Value, String> {
    let envelope: serde_json::Value = serde_json::from_str(command_json)
        .map_err(|error| format!("invalid command_json: {}", error))?;
    let (_, _, _, data) = dispatch_data_from_envelope(&envelope)?;
    Ok(data)
}
