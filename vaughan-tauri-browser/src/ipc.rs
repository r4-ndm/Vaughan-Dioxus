use std::time::Duration;

use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::traits::tokio::Stream as _;
use interprocess::local_socket::ToFsName;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use vaughan_ipc_types::{
    Handshake, IpcEnvelope, IpcRequest, IpcResponse, ValidationError, IPC_VERSION,
};

#[derive(Debug, thiserror::Error)]
pub enum IpcClientError {
    #[error("connect failed: {0}")]
    Connect(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("timeout")]
    Timeout,
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("unexpected response")]
    UnexpectedResponse,
}

/// Simple newline-delimited JSON IPC client.
///
/// This is intentionally sequential: one request at a time.
pub struct IpcClient {
    reader: BufReader<LocalSocketStream>,
    read_buf: String,
}

impl IpcClient {
    pub async fn connect(
        endpoint: &str,
        token: &str,
        op_timeout: Duration,
    ) -> Result<Self, IpcClientError> {
        #[cfg(windows)]
        let name = endpoint
            .to_fs_name::<interprocess::os::windows::local_socket::NamedPipe>()
            .map_err(|e| IpcClientError::Connect(e.to_string()))?;

        #[cfg(unix)]
        let name = endpoint
            .to_fs_name::<interprocess::os::unix::local_socket::FilesystemUdSocket>()
            .map_err(|e| IpcClientError::Connect(e.to_string()))?;

        let stream = timeout(op_timeout, LocalSocketStream::connect(name))
            .await
            .map_err(|_| IpcClientError::Timeout)?
            .map_err(|e| IpcClientError::Connect(e.to_string()))?;

        let mut client = Self {
            reader: BufReader::new(stream),
            read_buf: String::new(),
        };

        client.handshake(token, op_timeout).await?;

        Ok(client)
    }

    async fn handshake(&mut self, token: &str, op_timeout: Duration) -> Result<(), IpcClientError> {
        let hs = Handshake {
            version: IPC_VERSION,
            token: token.to_string(),
        };
        hs.validate()?;

        // Send handshake line.
        let line = serde_json::to_string(&hs).map_err(|e| IpcClientError::Serde(e.to_string()))?;
        let stream = self.reader.get_mut();
        timeout(op_timeout, stream.write_all(format!("{line}\n").as_bytes()))
            .await
            .map_err(|_| IpcClientError::Timeout)?
            .map_err(|e| IpcClientError::Io(e.to_string()))?;

        // Expect server to echo/ack handshake (same struct).
        let ack: Handshake = self.read_line_json(op_timeout).await?;
        ack.validate()?;
        Ok(())
    }

    pub async fn request(
        &mut self,
        id: u64,
        req: IpcRequest,
        op_timeout: Duration,
    ) -> Result<IpcEnvelope<IpcResponse>, IpcClientError> {
        req.validate()?;

        let env = IpcEnvelope { id, body: req };
        let line = serde_json::to_string(&env).map_err(|e| IpcClientError::Serde(e.to_string()))?;
        let stream = self.reader.get_mut();
        timeout(op_timeout, stream.write_all(format!("{line}\n").as_bytes()))
            .await
            .map_err(|_| IpcClientError::Timeout)?
            .map_err(|e| IpcClientError::Io(e.to_string()))?;

        let resp: IpcEnvelope<IpcResponse> = self.read_line_json(op_timeout).await?;
        if resp.id != id {
            return Err(IpcClientError::UnexpectedResponse);
        }
        Ok(resp)
    }

    async fn read_line_json<T: serde::de::DeserializeOwned>(
        &mut self,
        op_timeout: Duration,
    ) -> Result<T, IpcClientError> {
        self.read_buf.clear();
        timeout(op_timeout, self.reader.read_line(&mut self.read_buf))
            .await
            .map_err(|_| IpcClientError::Timeout)?
            .map_err(|e| IpcClientError::Io(e.to_string()))?;

        let s = self.read_buf.trim_end_matches(['\r', '\n']);
        serde_json::from_str(s).map_err(|e| IpcClientError::Serde(e.to_string()))
    }
}
