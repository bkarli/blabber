use blabber_core::channel::VoiceChannel;
use tauri::State;
use crate::AppState;


#[tauri::command]
pub async fn start_call(state: State<'_, AppState>, peer_endpoint_id: String)->Result<(),String>{
    let peer_id: iroh::EndpointId = peer_endpoint_id.parse().map_err(|e| format!("invalid peer id: {e}"))?;
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not created yet")?;
    let call = node.call(peer_id).await.map_err(|e| format!("{e:#}"))?;
    *state.active_call.lock().unwrap() = Some(call);
    Ok(())
}


#[tauri::command]
pub fn hang_up(state: State <'_, AppState>)->Result<(), String>{
    if let Some(call) = state.active_call.lock().unwrap().take(){
        call.hang_up();}
    if let Some(handle) = state.incoming_call_handle.lock().unwrap().take(){
        handle.hang_up();}
    Ok(())
}

#[tauri::command]
pub async fn my_endpoint_id(state: State<'_, AppState>) -> Result<String, String> {
    let node_guard = state.node.lock().await;
    let node = node_guard.as_ref().ok_or("Node not started yet")?;
    let endpoint = node.endpoint.as_ref().ok_or("Endpoint not created")?;
    Ok(endpoint.id().to_string())
}

#[tauri::command]
pub fn answer_call(state: State<'_, AppState>, accept: bool) -> Result<(), String> {
    if let Some(tx) = state.pending_call.lock().unwrap().take() {
        let _ = tx.send(accept);
    }
    Ok(())
}
