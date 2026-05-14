use anyhow::{Result, anyhow};
use ssh::{LocalSession, algorithm};
use std::net::TcpStream;

pub trait SSHConnector {
    fn execute(&mut self, command: &str) -> Result<String>;
}

pub struct SSHClient {
    session: LocalSession<TcpStream>,
}

impl SSHConnector for SSHClient {
    fn execute(&mut self, command: &str) -> Result<String> {
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

impl SSHClient {
    pub(crate) fn prepare_address(host: &str) -> String {
        if host.starts_with('[') {
            if host.contains("]:") {
                host.to_string()
            } else {
                format!("{}:22", host)
            }
        } else {
            let colons = host.chars().filter(|&c| c == ':').count();
            if colons > 1 {
                // IPv6 address without brackets and without port
                format!("[{}]:22", host)
            } else if colons == 1 {
                // IPv4:port or hostname:port
                host.to_string()
            } else {
                // IPv4 or hostname without port
                format!("{}:22", host)
            }
        }
    }

    pub fn connect(host: &str, username: &str, password: &str) -> Result<Self> {
        let full_addr = Self::prepare_address(host);
        let session = ssh::create_session()
            .username(username)
            .password(password)
            .add_kex_algorithms(algorithm::Kex::Curve25519Sha256)
            .add_pubkey_algorithms(algorithm::PubKey::SshEd25519)
            .connect(full_addr)
            .map_err(|e| anyhow!("Connection failed: {}", e))?
            .run_local();

        Ok(Self { session })
    }
}

#[cfg(test)]
pub struct MockSSHClient {
    pub responses: std::collections::HashMap<String, String>,
}

#[cfg(test)]
impl SSHConnector for MockSSHClient {
    fn execute(&mut self, command: &str) -> Result<String> {
        if let Some(resp) = self.responses.get(command) {
            Ok(resp.clone())
        } else {
            Err(anyhow!("Mock response not found for command: {}", command))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            SSHClient::prepare_address("192.168.88.1"),
            "192.168.88.1:22"
        );
        assert_eq!(
            SSHClient::prepare_address("192.168.88.1:2222"),
            "192.168.88.1:2222"
        );
        assert_eq!(
            SSHClient::prepare_address("router.local"),
            "router.local:22"
        );
        assert_eq!(
            SSHClient::prepare_address("router.local:2222"),
            "router.local:2222"
        );
        assert_eq!(
            SSHClient::prepare_address("2001:db8::1"),
            "[2001:db8::1]:22"
        );
        assert_eq!(
            SSHClient::prepare_address("[2001:db8::1]:2222"),
            "[2001:db8::1]:2222"
        );
        assert_eq!(
            SSHClient::prepare_address("[2001:db8::1]"),
            "[2001:db8::1]:22"
        );
    }
}
