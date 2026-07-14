use blabber_core::channel::VoiceChannel;
use tauri::State;
use crate::AppState; //ev pfad ahpasse


#[tauri::command]
pub async fn start_call(state: State<'_, AppState>, peer_endpoint_id: String)->Result<(),String>{
    let peer_id: iroh::EndpointId = peer_endpoint_id.parse().map_err(|e| format!("invalid peer id: {e}"))?;

    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not created yet")?;
    let call = node.call(peer_id).await.map_err(|e| e.to_string())?;
    *state.active_call.lock().unwrap() = Some(call);
    Ok(())
}


#[tauri::command]
pub fn hang_up(state: State <'_, AppState>)->Result<(), String>{
    if let Some(call) = state.active_call.lock().unwrap().take(){
        call.hang_up();}
    Ok(())
}
