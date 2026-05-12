use anyhow::{Result, anyhow};
use ssh::{LocalSession, algorithm};
use std::net::TcpStream;

pub struct SSHClient {
    session: LocalSession<TcpStream>,
}

impl SSHClient {
    pub fn connect(host: &str, username: &str, password: &str) -> Result<Self> {
        let session = ssh::create_session()
            .username(username)
            .password(password)
            .add_kex_algorithms(algorithm::Kex::Curve25519Sha256)
            .add_pubkey_algorithms(algorithm::PubKey::SshEd25519)
            .connect(format!("{}:22", host))
            .map_err(|e| anyhow!("Connection failed: {}", e))?
            .run_local();

        Ok(Self { session })
    }

    pub fn execute(&mut self, command: &str) -> Result<String> {
        //dbg!(&command);

        let exec = self
            .session
            .open_exec()
            .map_err(|e| anyhow!("Failed to open exec channel: {}", e))?;
        let output = exec
            .send_command(command)
            .map_err(|e| anyhow!("Failed to send command: {}", e))?;

        let result =
            String::from_utf8(output).map_err(|e| anyhow!("Invalid UTF-8 output: {}", e))?;

        //dbg!(&result);

        Ok(result)
    }
}
