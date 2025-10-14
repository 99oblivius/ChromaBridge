/// IPC protocol for communication between tray service and GUI
use anyhow::{Result, Context};
use crossbeam_channel::{Sender, Receiver, unbounded, RecvTimeoutError};
use serde::Serialize;
use std::thread;
use windows::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{CreateNamedPipeW, PIPE_TYPE_MESSAGE, PIPE_READMODE_MESSAGE, PIPE_WAIT, ConnectNamedPipe};
use windows::Win32::Foundation::CloseHandle;
use color_interlacer_core::{log_info, log_error, log_warn, GuiMessage, TrayMessage};

// Re-export types from core for convenience
pub use color_interlacer_core::{GuiMessage as GuiMsg, TrayMessage as TrayMsg};

/// Pipe name for IPC communication
pub const PIPE_NAME: &str = r"\\.\pipe\color-interlacer-tray";

/// Maximum message size (64KB)
const MAX_MESSAGE_SIZE: usize = 65536;

/// Send-safe wrapper for HANDLE (Windows pipe handles are thread-safe for concurrent I/O)
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}

impl SendHandle {
    fn get(&self) -> HANDLE {
        self.0
    }
}

/// IPC Server (runs in tray service)
pub struct IpcServer {
    message_rx: Receiver<GuiMessage>,
    command_tx: Sender<TrayMessage>,
    _server_thread: thread::JoinHandle<()>,
}

impl IpcServer {
    /// Start IPC server in background thread
    pub fn start() -> Result<Self> {
        let (message_tx, message_rx) = unbounded();
        let (command_tx, command_rx) = unbounded();

        let server_thread = thread::spawn(move || {
            if let Err(e) = run_server_loop(message_tx, command_rx) {
                log_error!("IPC server error: {}", e);
            }
        });

        log_info!("IPC server started");

        Ok(Self {
            message_rx,
            command_tx,
            _server_thread: server_thread,
        })
    }

    /// Try to receive a message (non-blocking)
    pub fn try_recv(&self) -> Option<GuiMessage> {
        self.message_rx.try_recv().ok()
    }

    /// Send a command to the connected GUI (non-blocking, queues message)
    pub fn send(&self, message: TrayMessage) -> Result<()> {
        self.command_tx.send(message)
            .map_err(|e| anyhow::anyhow!("Failed to queue command: {}", e))
    }
}

/// Server loop that accepts connections and reads messages
fn run_server_loop(message_tx: Sender<GuiMessage>, command_rx: Receiver<TrayMessage>) -> Result<()> {
    loop {
        // Create named pipe
        let pipe_name_wide: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let pipe_handle = CreateNamedPipeW(
                windows::core::PCWSTR(pipe_name_wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1, // Max instances
                MAX_MESSAGE_SIZE as u32,
                MAX_MESSAGE_SIZE as u32,
                0,
                None,
            );

            if pipe_handle == INVALID_HANDLE_VALUE {
                log_error!("Failed to create named pipe");
                continue;
            }

            log_info!("Waiting for GUI connection...");

            // Wait for client connection
            if ConnectNamedPipe(pipe_handle, None).is_err() {
                CloseHandle(pipe_handle).ok();
                continue;
            }

            log_info!("GUI connected");

            // Handle client connection (bidirectional)
            if let Err(e) = handle_client(pipe_handle, &message_tx, &command_rx) {
                log_warn!("Client disconnected: {}", e);
            }

            CloseHandle(pipe_handle).ok();
        }
    }
}

/// Handle a connected client (bidirectional communication)
fn handle_client(pipe: HANDLE, message_tx: &Sender<GuiMessage>, command_rx: &Receiver<TrayMessage>) -> Result<()> {
    // Create a channel to signal the write thread to stop
    let (stop_tx, stop_rx) = unbounded();

    // Wrap HANDLE for safe transfer to thread
    let pipe_handle = SendHandle(pipe);

    // Spawn writer thread for sending commands to GUI
    let command_rx_clone = command_rx.clone();
    let write_thread = thread::spawn(move || {
        let pipe = pipe_handle.get();
        loop {
            // Check if we should stop (non-blocking)
            if stop_rx.try_recv().is_ok() {
                break;
            }

            // Try to receive a command (with timeout)
            match command_rx_clone.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(command) => {
                    if let Err(e) = send_message(pipe, &command) {
                        log_error!("Failed to send command to GUI: {}", e);
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // Main read loop
    let read_result = (|| -> Result<()> {
        loop {
            // Read message length (4 bytes)
            let mut len_buf = [0u8; 4];
            let mut bytes_read = 0u32;

            unsafe {
                if ReadFile(
                    pipe,
                    Some(&mut len_buf),
                    Some(&mut bytes_read),
                    None,
                ).is_err() {
                    return Err(anyhow::anyhow!("Failed to read message length"));
                }
            }

            if bytes_read == 0 {
                return Err(anyhow::anyhow!("Connection closed"));
            }

            let msg_len = u32::from_le_bytes(len_buf) as usize;
            if msg_len > MAX_MESSAGE_SIZE {
                return Err(anyhow::anyhow!("Message too large: {}", msg_len));
            }

            // Read message data
            let mut msg_buf = vec![0u8; msg_len];
            let mut bytes_read = 0u32;

            unsafe {
                if ReadFile(
                    pipe,
                    Some(&mut msg_buf),
                    Some(&mut bytes_read),
                    None,
                ).is_err() {
                    return Err(anyhow::anyhow!("Failed to read message data"));
                }
            }

            // Deserialize message
            let message: GuiMessage = serde_json::from_slice(&msg_buf)
                .context("Failed to deserialize message")?;

            log_info!("Received message: {:?}", message);

            // Forward to main thread
            message_tx.send(message).ok();
        }
    })();

    // Stop writer thread
    let _ = stop_tx.send(());
    let _ = write_thread.join();

    read_result
}

/// Send a message through a pipe
fn send_message<T: Serialize>(pipe: HANDLE, message: &T) -> Result<()> {
    // Serialize message
    let msg_data = serde_json::to_vec(message)?;
    let msg_len = msg_data.len() as u32;

    if msg_len > MAX_MESSAGE_SIZE as u32 {
        return Err(anyhow::anyhow!("Message too large"));
    }

    // Write length prefix
    let len_bytes = msg_len.to_le_bytes();
    let mut bytes_written = 0u32;

    unsafe {
        WriteFile(
            pipe,
            Some(&len_bytes),
            Some(&mut bytes_written),
            None,
        )?;

        // Write message data
        WriteFile(
            pipe,
            Some(&msg_data),
            Some(&mut bytes_written),
            None,
        )?;
    }

    log_info!("Sent message through pipe");
    Ok(())
}

/// IPC Client (runs in GUI)
pub struct IpcClient {
    pipe: HANDLE,
}

impl IpcClient {
    /// Connect to tray service
    pub fn connect() -> Result<Self> {
        use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL};

        let pipe_name_wide: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let pipe = CreateFileW(
                windows::core::PCWSTR(pipe_name_wide.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?;

            log_info!("Connected to tray service");

            Ok(Self { pipe })
        }
    }

    /// Send a message to tray service
    pub fn send(&self, message: &GuiMessage) -> Result<()> {
        send_message(self.pipe, message)
    }

    /// Receive a message from tray service (blocking)
    pub fn recv(&self) -> Result<TrayMessage> {
        // Read message length (4 bytes)
        let mut len_buf = [0u8; 4];
        let mut bytes_read = 0u32;

        unsafe {
            ReadFile(
                self.pipe,
                Some(&mut len_buf),
                Some(&mut bytes_read),
                None,
            )?;
        }

        if bytes_read == 0 {
            return Err(anyhow::anyhow!("Connection closed"));
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        if msg_len > MAX_MESSAGE_SIZE {
            return Err(anyhow::anyhow!("Message too large: {}", msg_len));
        }

        // Read message data
        let mut msg_buf = vec![0u8; msg_len];
        let mut bytes_read = 0u32;

        unsafe {
            ReadFile(
                self.pipe,
                Some(&mut msg_buf),
                Some(&mut bytes_read),
                None,
            )?;
        }

        // Deserialize message
        let message: TrayMessage = serde_json::from_slice(&msg_buf)
            .context("Failed to deserialize tray message")?;

        log_info!("Received tray command: {:?}", message);

        Ok(message)
    }
}

impl Drop for IpcClient {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.pipe).ok();
        }
    }
}
