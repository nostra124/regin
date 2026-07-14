//! Terminal WebSocket (FEAT-087, acceptance criterion 7): a real PTY
//! session (`portable-pty`) bridged to the browser over JSON WS frames.
//! `{"type":"input","data":"..."}` writes keystrokes into the PTY,
//! `{"type":"resize","cols":N,"rows":N}` propagates a resize;
//! `{"type":"output","data":"..."}` streams PTY output back,
//! `{"type":"exit","code":N}` signals the shell exited.
//!
//! `portable-pty`'s reader/writer are blocking `std::io::{Read,Write}`, not
//! async — both sides run on dedicated blocking threads bridged to the
//! WS's async task via `tokio::sync::mpsc` channels, the standard pattern
//! for wrapping a blocking I/O source in an async server.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::{AuthedUser, SharedState, WebuiState};

pub async fn handler(ws: WebSocketUpgrade, State(state): SharedState, _user: AuthedUser) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run(socket, state))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

async fn run(mut socket: WebSocket, _state: Arc<WebuiState>) {
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }) {
        Ok(p) => p,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({"type": "error", "message": format!("failed to open pty: {e:#}")}).to_string().into())).await;
            return;
        }
    };

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");
    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({"type": "error", "message": format!("failed to spawn shell: {e:#}")}).to_string().into())).await;
            return;
        }
    };
    drop(pair.slave);

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({"type": "error", "message": format!("failed to clone pty reader: {e:#}")}).to_string().into())).await;
            return;
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({"type": "error", "message": format!("failed to take pty writer: {e:#}")}).to_string().into())).await;
            return;
        }
    };

    // PTY -> WS: a blocking reader thread feeds an async channel.
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // WS -> PTY: keystrokes/resizes handed to a dedicated writer thread
    // (writer isn't `Send`-safe to call from multiple async tasks, and a
    // blocking `write` shouldn't run inline on the async task anyway).
    let (in_tx, mut in_rx) = mpsc::channel::<Vec<u8>>(64);
    let master = pair.master;
    std::thread::spawn(move || {
        while let Some(bytes) = in_rx.blocking_recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    loop {
        tokio::select! {
            chunk = out_rx.recv() => {
                match chunk {
                    Some(bytes) => {
                        let data = String::from_utf8_lossy(&bytes).into_owned();
                        if socket.send(Message::Text(json!({"type": "output", "data": data}).to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let code = child.try_wait().ok().flatten().map(|s| s.exit_code()).unwrap_or(0);
                        let _ = socket.send(Message::Text(json!({"type": "exit", "code": code}).to_string().into())).await;
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMsg>(&text) {
                            Ok(ClientMsg::Input { data }) => {
                                if in_tx.send(data.into_bytes()).await.is_err() {
                                    break;
                                }
                            }
                            Ok(ClientMsg::Resize { cols, rows }) => {
                                let _ = master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_pty_can_run_a_command_and_produce_output() {
        // Exercises the real `portable-pty` machinery this module wraps
        // (not the WS glue, which needs a live axum server — see
        // `mod.rs`'s integration test) — a short-lived `echo` through a
        // real pty confirms the read/write plumbing itself works in this
        // sandbox.
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut cmd = CommandBuilder::new("echo");
        cmd.arg("hello-from-pty");
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().unwrap();
        drop(pair.master);

        let mut output = Vec::new();
        let mut buf = [0u8; 4096];
        // A pty stays open (the master side) until every clone of the
        // writer/child is gone, so read in a loop with a generous but
        // bounded number of iterations rather than relying on EOF alone.
        for _ in 0..50 {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
            if child.try_wait().ok().flatten().is_some() && output.windows(9).any(|w| w == b"hello-fro") {
                break;
            }
        }
        let _ = child.wait();

        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("hello-from-pty"), "expected pty output to contain the echoed text, got: {text:?}");
    }
}
