use std::rc::Rc;
use std::cell::RefCell;
use std::borrow::Borrow;
use bytes::BytesMut;

#[derive(Clone)]
pub struct Arena {
    buf: BytesMut,
}

impl Arena {
    fn get_node_hdr(&self, index: usize) -> &NodeHdr {
        debug_assert!(index <= self.buf.len() - std::mem::size_of::<NodeHdr>());
        unsafe { std::mem::transmute::<&u8, &NodeHdr>(&self.buf[index]) }
    }
}

pub struct Node {
    arena: Arena,
    index: usize,
}

#[repr(u8)]
pub enum NodeKind {
    Null        = 0u8,
    Boolean     = 1u8,
    Integer     = 2u8,
    Float       = 3u8,
    String      = 4u8,
    Array       = 5u8,
    Object      = 6u8,
}

impl From<u8> for NodeKind {
    fn from(value: u8) -> Self {
        match value {
            0 => NodeKind::Null,
            1 => NodeKind::Boolean,
            2 => NodeKind::Integer,
            3 => NodeKind::Float,
            4 => NodeKind::String,
            5 => NodeKind::Array,
            6 => NodeKind::Object,
            _ => std::panic!("invalid NodeKind value"),
        }
    }
}

#[repr(packed)]
struct NodeHdr {
    parent_offset: usize,
    child_index: usize,
}

impl NodeHdr {
    fn parent_offset(&self) -> usize {
        self.parent_offset & 0xFFFF_FFFF_FFFF_FFF8
    }

    fn kind(&self) -> NodeKind {
        NodeKind::from((self.parent_offset & 7) as u8)
    }

    fn child_index(&self) -> usize {
        self.child_index
    }
}

impl Node {
    pub fn is_root(&self) -> bool {
        self.index == 0
    }

    fn hdr(&self) -> &NodeHdr {
        self.arena.get_node_hdr(self.index)
    }

    fn size(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn run() {
        
    }
}