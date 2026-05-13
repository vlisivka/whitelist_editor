use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use ipnet::Ipv4Net;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Lease {
    pub address: Option<String>,
    pub mac_address: String,
    pub client_id: Option<String>,
    pub server: String,
    pub comment: Option<String>,
    pub block_access: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DhcpServer {
    pub name: String,
    pub comment: Option<String>,
    pub interface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DhcpNetwork {
    pub address: String, // CIDR
    pub comment: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DhcpData {
    pub servers: Vec<DhcpServer>,
    pub networks: Vec<DhcpNetwork>,
    pub leases: Vec<Lease>,
}

pub fn parse_all(input: &str) -> DhcpData {
    let mut data = DhcpData::default();
    let normalized = input.replace("\\\n", "").replace("\\\r\n", "");
    let mut current_section = "";

    let re_kv = Regex::new(r#"(?P<key>[\w-]+)=\s*(?P<val>"[^"]*"|\S+)"#).unwrap();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.to_lowercase() == "/ip dhcp-server" {
            current_section = "server";
            continue;
        } else if trimmed.to_lowercase() == "/ip dhcp-server lease" {
            current_section = "lease";
            continue;
        } else if trimmed.to_lowercase() == "/ip dhcp-server network" {
            current_section = "network";
            continue;
        } else if trimmed.starts_with('/') {
            current_section = "";
            continue;
        }

        if trimmed.starts_with("add ") {
            match current_section {
                "server" => {
                    let mut s = DhcpServer::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                        }
                        match key {
                            "name" => s.name = val,
                            "comment" => s.comment = Some(val),
                            "interface" => s.interface = val,
                            _ => {}
                        }
                    }
                    if !s.name.is_empty() {
                        data.servers.push(s);
                    }
                }
                "lease" => {
                    let mut l = Lease::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                        }
                        match key {
                            "address" => l.address = Some(val),
                            "mac-address" => {
                                // Sometimes mac-address has extra spaces in export
                                l.mac_address = val.trim().to_string();
                            }
                            "client-id" => l.client_id = Some(val),
                            "server" => l.server = val,
                            "comment" => l.comment = Some(val),
                            "block-access" => l.block_access = val == "yes",
                            _ => {}
                        }
                    }
                    if !l.mac_address.is_empty() {
                        data.leases.push(l);
                    }
                }
                "network" => {
                    let mut n = DhcpNetwork::default();
                    for cap in re_kv.captures_iter(trimmed) {
                        let key = &cap["key"];
                        let mut val = cap["val"].to_string();
                        if val.starts_with('"') && val.ends_with('"') {
                            val = val[1..val.len() - 1].to_string();
                        }
                        match key {
                            "address" => n.address = val,
                            "comment" => n.comment = Some(val),
                            "gateway" => n.gateway = Some(val),
                            _ => {}
                        }
                    }
                    if !n.address.is_empty() {
                        data.networks.push(n);
                    }
                }
                _ => {}
            }
        }
    }
    data
}

pub fn find_network_for_server<'a>(
    server: &DhcpServer,
    networks: &'a [DhcpNetwork],
) -> Option<&'a DhcpNetwork> {
    // Priority 1: Match by comment
    if let Some(s_comment) = &server.comment {
        if let Some(net) = networks.iter().find(|n| n.comment.as_ref() == Some(s_comment)) {
            return Some(net);
        }
    }
    
    // Priority 2: Fallback (can't really match by interface easily without more data)
    // In many cases, people use the same name or comment.
    // If no comment, maybe try matching server name with network comment?
    if let Some(net) = networks.iter().find(|n| n.comment.as_ref() == Some(&server.name)) {
        return Some(net);
    }

    None
}

pub fn is_ip_in_range(ip: &str, network: &DhcpNetwork) -> bool {
    let ip_parsed: Ipv4Addr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    let net_parsed: Ipv4Net = match network.address.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    net_parsed.contains(&ip_parsed)
}

pub fn find_first_free_ip(
    network: &DhcpNetwork,
    existing_leases: &[Lease],
) -> Option<String> {
    let net_parsed: Ipv4Net = network.address.parse().ok()?;
    let gateway: Option<Ipv4Addr> = network.gateway.as_ref().and_then(|g| g.parse().ok());
    
    let mut taken_ips: std::collections::HashSet<Ipv4Addr> = existing_leases
        .iter()
        .filter_map(|l| l.address.as_ref()?.parse().ok())
        .collect();
    
    if let Some(g) = gateway {
        taken_ips.insert(g);
    }

    // ipnet hosts() returns usable hosts (skips network/broadcast)
    for ip in net_parsed.hosts() {
        if !taken_ips.contains(&ip) {
            return Some(ip.to_string());
        }
    }
    None
}

pub fn is_ip_unique(ip: &str, existing_leases: &[Lease], current_mac: &str) -> bool {
    let ip_val = ip.trim();
    if ip_val.is_empty() {
        return true;
    }

    !existing_leases.iter().any(|l| {
        l.address.as_deref() == Some(ip_val) && l.mac_address != current_mac
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIKROTIK_EXPORT: &str = r#"
/ip dhcp-server
add add-arp=yes comment=guest interface=GSTVLAN name=guest-dhcp
add add-arp=yes comment=corp interface=CRPVLAN lease-time=1h name=corp-dhcp
add add-arp=yes comment=manage interface=MNGVLAN name=mng-server
/ip dhcp-server lease
add address=172.16.20.217 block-access=yes mac-address=A4:C6:9A:08:86:C8 server=corp-dhcp
add address=172.22.2.29 comment=029SYN mac-address=F4:1E:57:7F:D1:57 server=mng-server
/ip dhcp-server network
add address=172.16.20.0/23 comment=corp dns-server=172.16.20.1 gateway=172.16.20.1
add address=172.22.2.0/24 comment=manage dns-server=172.22.2.2 gateway=172.22.2.1
add address=192.168.10.0/24 comment=guest dns-server=192.168.10.1 gateway=192.168.10.1
"#;

    #[test]
    fn test_parse_all() {
        let data = parse_all(MIKROTIK_EXPORT);
        
        assert_eq!(data.servers.len(), 3);
        assert_eq!(data.servers[0].name, "guest-dhcp");
        assert_eq!(data.servers[0].comment.as_deref(), Some("guest"));

        assert_eq!(data.leases.len(), 2);
        assert_eq!(data.leases[0].address.as_deref(), Some("172.16.20.217"));
        assert_eq!(data.leases[0].server, "corp-dhcp");

        assert_eq!(data.networks.len(), 3);
        assert_eq!(data.networks[0].address, "172.16.20.0/23");
        assert_eq!(data.networks[0].comment.as_deref(), Some("corp"));
    }

    #[test]
    fn test_match_server_network() {
        let data = parse_all(MIKROTIK_EXPORT);
        let corp_server = data.servers.iter().find(|s| s.name == "corp-dhcp").unwrap();
        
        let matched = find_network_for_server(corp_server, &data.networks);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().address, "172.16.20.0/23");
    }

    #[test]
    fn test_ip_in_range() {
        let net = DhcpNetwork { address: "172.16.20.0/23".into(), ..Default::default() };
        assert!(is_ip_in_range("172.16.20.5", &net));
        assert!(is_ip_in_range("172.16.21.254", &net));
        assert!(!is_ip_in_range("172.16.22.1", &net));
    }

    #[test]
    fn test_find_first_free_ip() {
        let net = DhcpNetwork { 
            address: "172.22.2.0/24".into(), 
            gateway: Some("172.22.2.1".into()),
            ..Default::default() 
        };
        let existing = vec![
            Lease { address: Some("172.22.2.2".to_string()), ..Default::default() },
            Lease { address: Some("172.22.2.3".to_string()), ..Default::default() },
        ];
        // Should skip .0 (network), .1 (gateway), .2 (existing), .3 (existing)
        // So .4 should be free
        let free = find_first_free_ip(&net, &existing);
        assert_eq!(free.as_deref(), Some("172.22.2.4"));
    }
}
