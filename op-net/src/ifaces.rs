use interfaces;
use interfaces::{Interface, NextHop};
use interfaces::flags::*;

use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};

pub fn get_broadcasts() -> Result<Vec<SocketAddr>, interfaces::InterfacesError> {
    let up_br = Interface::get_all()?;

    let up_br = up_br.iter()
        .filter(|ifc| {
            ifc.flags.contains(IFF_UP | IFF_BROADCAST)
        })
        .map(|ifc| {
            ifc.addresses
                .iter()
                .filter_map(|addr| {
                    if let Some(_) = addr.hop {
                        Some(&addr.hop)
                    } else {
                        None
                    }
                })
                .filter_map(|br_addr| {
                    if let Some(ref hop) = *br_addr {
                        match *hop {
                            NextHop::Broadcast(ref br) => Some(br.clone()),
                            _ => None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .fold(vec![], |mut a, mut b| {
            a.append(&mut b);
            a
        });
    Ok(up_br)
}

pub fn get_addr_br_tups() -> Result<Vec<(SocketAddr, SocketAddr)>, interfaces::InterfacesError> {
    let up_br = Interface::get_all()?;

    let up_br = up_br.iter()
        .filter(|ifc| {
            ifc.flags.contains(IFF_UP | IFF_BROADCAST)
        })
        .map(|ifc| {
            ifc.addresses
                .iter()
                .filter(|addr| {
                    if let Some(_) = addr.hop {
                        true
                    } else {
                        false
                    }
                })
                .filter(|addr| {
                    if let Some(_) = addr.addr {
                        true
                    } else {
                        false
                    }
                })
                .filter_map(|addr| {
                    let hop = match addr.hop {
                        Some(ref hop) => hop,
                        _ => return None
                    };

                    let addr = addr.addr.unwrap();

                    let br = match *hop {
                        NextHop::Broadcast(ref br) => br.clone(),
                        _ => return None
                    };

                    Some((addr, br))
                })
                .collect::<Vec<_>>()
        })
        .fold(vec![], |mut a, mut b| {
            a.append(&mut b);
            a
        });
    Ok(up_br)
}

///Returns mac address for socket addr
pub fn get_mac(socket: &SocketAddr) -> Result<Option<[u8; 6]>, interfaces::InterfacesError> {
    let mut socket = socket.clone();
    // need for comparison
    socket.set_port(0);

    let all_addrs = Interface::get_all()?;
    let ifc = all_addrs.iter()
        .find(|ifc| {
            for addr in ifc.addresses.iter() {
                if let Some(ref a) = addr.addr {
                    if a == &socket {
                        return true;
                    }
                }
            }
            false
        });
    match ifc {
        Some(ifc) => {
            let hw_addr = ifc.hardware_addr()?;
            let hw_addr = hw_addr.as_bytes();
            let mut addr = [0; 6];
            for i in 0..6 {
                addr[i] = hw_addr[i];
            }


            Ok(Some(addr))
        }
        None => Ok(None)
    }
}

/// returns all up and multicast  ipv4 interfaces
pub fn get_up_ipv4_multicast() -> Result<Vec<SocketAddrV4>, interfaces::InterfacesError> {
    let up_ipv4 = Interface::get_all()?;

    let up_ipv4 = up_ipv4.iter()
        .filter(|ifc| {
            ifc.flags.contains(IFF_UP | IFF_MULTICAST)
        })
        .map(|ifc| {
            ifc.addresses
                .iter()
                .filter_map(|addr| {
                    if let Some(a) = addr.addr {
                        Some(a)
                    } else {
                        None
                    }
                })
                .filter(|addr| addr.is_ipv4())
                .map(|addr| match addr {
                    SocketAddr::V4(addr) => addr,
                    _ => panic!()
                })
                .collect::<Vec<_>>()
        })
        .fold(vec![], |mut a, mut b| {
            a.append(&mut b);
            a
        });
    Ok(up_ipv4)
}

/// returns all up ipv4 addresses without loopback
pub fn get_up_ipv4() -> Result<Vec<SocketAddrV4>, interfaces::InterfacesError> {
    let up_ipv4 = Interface::get_all()?;

    let up_ipv4 = up_ipv4.iter()
        .filter(|ifc| {
            ifc.flags.contains(IFF_UP) && !ifc.is_loopback()
        })
        .map(|ifc| {
            ifc.addresses
                .iter()
                .filter_map(|addr| {
                    if let Some(a) = addr.addr {
                        Some(a)
                    } else {
                        None
                    }
                })
                .filter(|addr| addr.is_ipv4())
                .map(|addr| match addr {
                    SocketAddr::V4(addr) => addr,
                    _ => panic!()
                })
                .collect::<Vec<_>>()
        })
        .fold(vec![], |mut a, mut b| {
            a.append(&mut b);
            a
        });
    Ok(up_ipv4)
}

/// returns all up and broadcast  ipv6 interfaces
pub fn get_up_ipv6_multicast() -> Result<Vec<SocketAddrV6>, interfaces::InterfacesError> {
    let up_ipv4 = Interface::get_all()?;

    let up_ipv4 = up_ipv4.iter()
        .filter(|ifc| {
            ifc.flags.contains(IFF_UP | IFF_MULTICAST)
        })
        .map(|ifc| {
            ifc.addresses
                .iter()
                .filter_map(|addr| {
                    if let Some(a) = addr.addr {
                        Some(a)
                    } else {
                        None
                    }
                })
                .filter(|addr| addr.is_ipv6())
                .map(|addr| match addr {
                    SocketAddr::V6(addr) => addr,
                    _ => panic!()
                })
                .collect::<Vec<_>>()
        })
        .fold(vec![], |mut a, mut b| {
            a.append(&mut b);
            a
        });
    Ok(up_ipv4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_br_test() {
        for br_addr in get_broadcasts() {
            println!("{:#?}", br_addr);
        }
    }

    #[test]
    fn get_mac_test() {
        let mac = get_mac(&"192.168.124.1:0".parse().unwrap()).unwrap();
        eprintln!("mac = {:?}", mac);
    }

    #[test]
    fn get_ipv4_multicast_test() {
        for br_addr in get_up_ipv4_multicast() {
            println!("{:#?}", br_addr);
        }
    }    #[test]
    fn get_ipv4_test() {
        for br_addr in get_up_ipv4() {
            println!("{:#?}", br_addr);
        }
    }
    #[test]
    fn get_ipv6_multicast_test() {
        for br_addr in get_up_ipv6_multicast() {
            println!("{:#?}", br_addr);
        }
    }

    #[test]
    fn get_addr_br_tups_test() {
        for tup in get_addr_br_tups() {
            println!("{:#?}", tup);
        }
    }
}
