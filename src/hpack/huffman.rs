use super::huffman_codes::*;

pub fn decode(
    b: *const u8,
    e: *const u8,
) -> Result<Vec<u8>, &'static str> {
    let mut iter = BitIterator::new(b, e);
    let tree = HuffmanTree::new();
    let mut walker = HuffmanTreeWalker::new(&*tree);
    let mut res = vec!();
    for x in iter {
        let c = walker.advance(x);
        match c {
            None => (),
            Some(c) => {
                match c {
                    Char::Normal(c) => {
                        res.push(c);
                    },
                    _ => {
                        return Err("decode error on Huffman compressed headers.");
                    }
                }
            }
        }
    }
    if !walker.is_root() {
        loop {
            let c = walker.advance(1);
            match c {
                None => (),
                Some(c) => {
                    match c {
                        Char::EoS => {
                            break;
                        },
                        _ => {
                            return Err("decode error on Huffman compressed headers.");
                        }
                    }
                }
            }
        }
    }
    Ok(res)
}

struct BitIterator {
    cur: *const u8,
    end: *const u8,
    remaining_bits_in_cur_byte: usize,
    cur_byte: u8,
}

impl BitIterator {
    fn new(b: *const u8, e: *const u8) -> BitIterator {
        unsafe {
            BitIterator{
                cur: b,
                end: e,
                remaining_bits_in_cur_byte: 8,
                cur_byte: if b < e {*b} else {0}}
        }
    }
}

impl Iterator for BitIterator {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.cur >= self.end {
            return None
        }

        assert!(self.remaining_bits_in_cur_byte > 0);

        let res = self.cur_byte & 0x80;

        self.remaining_bits_in_cur_byte -= 1;
        if self.remaining_bits_in_cur_byte > 0 {
            self.cur_byte <<= 1;
        } else {
            unsafe {
                self.cur = self.cur.add(1);
            }
            if self.cur < self.end {
                self.cur_byte = unsafe {*self.cur};
                self.remaining_bits_in_cur_byte = 8;
            } else {
                // nothing should do
            }
        }

        Some(res)
    }
}

struct HuffmanTreeWalker<'a> {
    tree: &'a HuffmanTree,
    cur_node: *const TreeNode,
}

impl <'a>  HuffmanTreeWalker<'a> {
    fn new(tree: &'a HuffmanTree) -> HuffmanTreeWalker<'a> {
        HuffmanTreeWalker{
            tree,
            cur_node: tree.root}
    }

    fn advance(&mut self, bit: u8) -> Option<Char> {
        let cur_node = unsafe {self.cur_node.as_ref::<'a>()}.unwrap();
        let next = match cur_node {
            TreeNode::Leaf(_) => unreachable!(),
            TreeNode::Inner((left, right)) => {
                if bit == 0 {
                    left
                } else {
                    right
                }
            }
        };
        let next = unsafe {next.as_ref::<'a>()}.unwrap();
        match unsafe {next} {
            TreeNode::Leaf(c) => {
                self.cur_node = self.tree.root;
                Some(c.clone())
            },
            TreeNode::Inner(_) => {
                self.cur_node = next;
                None
            }
        }
    }

    fn is_root(&self) -> bool {
        self.cur_node == self.tree.root
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bit_iterator_0() {
        let buf: Vec<u8> = vec!();
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let mut iter = BitIterator::new(b, e);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_bit_iterator_1() {
        let oracle = 0xCCu8;
        let buf: Vec<u8> = vec!(oracle);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let mut iter = BitIterator::new(b, e);
        let mut trial = String::new();
        for x in iter {
            trial.push(if x > 0 {'1'} else {'0'});
        }
        assert_eq!(trial, format!("{:b}", oracle));
    }

    #[test]
    fn test_bit_iterator_2() {
        let oracle0 = 0xCCu8;
        let oracle1 = 0x55u8;
        let buf: Vec<u8> = vec!(oracle0, oracle1);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let mut iter = BitIterator::new(b, e);
        let mut trial = String::new();
        for x in iter {
            trial.push(if x > 0 {'1'} else {'0'});
        }
        assert_eq!(trial, format!("{:b}{:08b}", oracle0, oracle1));
    }

    #[test]
    fn test_huffman_tree_walker_0() {
        let buf = vec!(0xF8u8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let mut iter = BitIterator::new(b, e);
        let tree = HuffmanTree::new();
        let mut walker = HuffmanTreeWalker::new(&*tree);
        let mut trial: Vec<Char> = vec!();
        for x in iter {
            let c = walker.advance(x);
            if c.is_some() {
                trial.push(c.unwrap());
            }
        }
        assert_eq!(
            format!("{:?}", trial),
            "[Normal(38)]");
        assert!(walker.is_root());
    }

    #[test]
    fn test_huffman_tree_walker_1() {
        let buf = vec!(0x53u8, 0xF8u8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let mut iter = BitIterator::new(b, e);
        let tree = HuffmanTree::new();
        let mut walker = HuffmanTreeWalker::new(&*tree);
        let mut trial: Vec<Char> = vec!();
        for x in iter {
            let c = walker.advance(x);
            if c.is_some() {
                trial.push(c.unwrap());
            }
        }
        assert_eq!(
            format!("{:?}", trial),
            "[Normal(32), Normal(33)]");
        assert!(walker.is_root());
    }
}
