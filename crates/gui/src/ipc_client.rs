/// IPC client wrapper for GUI to communicate with tray service
use color_interlacer_core::{log_info, log_warn, GuiMessage, TrayMessage};
use windows::Win32::Foundation::{HANDLE, CloseHandle};

const PIPE_NAME: &str = r"\\.\pipe\color-interlacer-tray";
const MAX_MESSAGE_SIZE: usize = 65536;

/// Wrapper around IPC client with safe error handling
pub struct IpcClientWrapper {
    pipe: Option<HANDLE>,
}

impl IpcClientWrapper {
    /// Attempt to connect to tray service (non-fatal if it fails)
    pub fn connect() -> Self {
        match Self::try_connect() {
            Ok(pipe) => {
                log_info!("Connected to tray service");
                Self { pipe: Some(pipe) }
            }
            Err(e) => {
                log_warn!("Failed to connect to tray service: {} (GUI will work standalone)", e);
                Self { pipe: None }
            }
        }
    }

    fn try_connect() -> windows::core::Result<HANDLE> {
        use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL};

        let pipe_name_wide: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            CreateFileW(
                windows::core::PCWSTR(pipe_name_wide.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
    }

    /// Try to receive a message from tray service (non-blocking)
    pub fn try_recv(&self) -> Option<TrayMessage> {
        use windows::Win32::Storage::FileSystem::ReadFile;
        use windows::Win32::System::Pipes::PeekNamedPipe;

        if self.pipe.is_none() {
            return None;
        }

        let pipe = self.pipe.unwrap();

        // Read message length (4 bytes) - non-blocking check
        let mut len_buf = [0u8; 4];
        let mut bytes_read = 0u32;
        let mut available = 0u32;

        unsafe {
            // Check if data is available without blocking
            if PeekNamedPipe(
                pipe,
                None,
                0,
                None,
                Some(&mut available),
                None,
            ).is_err() || available < 4 {
                return None;
            }

            // Data is available, read it
            if ReadFile(
                pipe,
                Some(&mut len_buf),
                Some(&mut bytes_read),
                None,
            ).is_err() {
                log_warn!("Failed to read message length from tray");
                return None;
            }
        }

        if bytes_read == 0 {
            return None;
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        if msg_len > MAX_MESSAGE_SIZE {
            log_warn!("Message too large from tray: {}", msg_len);
            return None;
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
                log_warn!("Failed to read message data from tray");
                return None;
            }
        }

        // Deserialize message
        match serde_json::from_slice::<TrayMessage>(&msg_buf) {
            Ok(message) => {
                log_info!("Received tray command: {:?}", message);
                Some(message)
            }
            Err(e) => {
                log_warn!("Failed to deserialize tray message: {}", e);
                None
            }
        }
    }

    /// Send a message to tray service (non-fatal if it fails)
    pub fn send(&self, message: GuiMessage) {
        if let Some(pipe) = self.pipe {
            if let Err(e) = self.try_send(pipe, &message) {
                log_warn!("Failed to send message to tray: {}", e);
            }
        }
    }

    fn try_send(&self, pipe: HANDLE, message: &GuiMessage) -> anyhow::Result<()> {
        use windows::Win32::Storage::FileSystem::WriteFile;

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

        Ok(())
    }
}

impl Drop for IpcClientWrapper {
    fn drop(&mut self) {
        if let Some(pipe) = self.pipe {
            unsafe {
                CloseHandle(pipe).ok();
            }
        }
    }
}
