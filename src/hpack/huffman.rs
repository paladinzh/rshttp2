use super::huffman_codes::*;
use super::super::*;

pub fn decode(
    input: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let iter = BitIterator::new(input);
    let mut walker = HuffmanTreeWalker::new(&*HUFFMAN_TREE);
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

pub fn encode(out: &mut Vec<u8>, input: &[u8]) -> () {
    const TOTAL_BITS: usize = 64;
    const BYTE_WIDTH: usize = 8;
    let mut b = input.as_ptr();
    let e = unsafe {b.add(input.len())};
    let mut remaining_bits = 0usize;
    let mut buf = 0u64;
    loop {
        while remaining_bits >= BYTE_WIDTH {
            let head = chop_head(&mut buf);
            out.push(head);
            remaining_bits -= 8;
        }

        if b == e {
            break;
        }

        let c = unsafe {
            let c = *b;
            b = b.add(1);
            c
        };
        let lsb = RAW_TABLE[c as usize].lsb as u64;
        let bits = RAW_TABLE[c as usize].bits;
        buf |= lsb << (TOTAL_BITS - remaining_bits - bits);
        remaining_bits += bits;
    }

    if remaining_bits > 0 {
        assert!(remaining_bits < BYTE_WIDTH);
        let tail = (1u64 << (TOTAL_BITS - remaining_bits)) - 1;
        buf |= tail;
        let head = chop_head(&mut buf);
        out.push(head);
    }
}

const BYTE_WIDTH: usize = 8;

fn chop_head(buf: &mut u64) -> u8 {
    let res = (*buf >> 56) as u8;
    *buf <<= 8;
    res
}

struct BitIterator<'a> {
    remaining_buf: &'a [u8],
    remaining_bits_in_cur_byte: usize,
    cur_byte: u8,
}

impl<'a> BitIterator<'a> {
    fn new(input: &[u8]) -> BitIterator {
        BitIterator{
            remaining_buf: input,
            remaining_bits_in_cur_byte: 0,
            cur_byte: 0}
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.remaining_buf.is_empty() && self.remaining_bits_in_cur_byte == 0 {
            return None
        }

        if self.remaining_bits_in_cur_byte == 0 {
            assert!(!self.remaining_buf.is_empty());
            let (byte, buf) = self.remaining_buf.split_first().unwrap();
            self.cur_byte = *byte;
            self.remaining_bits_in_cur_byte = BYTE_WIDTH;
            self.remaining_buf = buf;
        }
        
        assert!(self.remaining_bits_in_cur_byte > 0);

        let res = self.cur_byte & 0x80;
        self.remaining_bits_in_cur_byte -= 1;
        self.cur_byte <<= 1;

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
        match next {
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
    use random::Source;
    use super::*;

    #[test]
    fn bit_iterator_0() {
        let buf: Vec<u8> = vec!();
        let mut iter = BitIterator::new(buf.as_slice());
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn bit_iterator_1() {
        let oracle = 0xCCu8;
        let buf: Vec<u8> = vec!(oracle);
        let iter = BitIterator::new(buf.as_slice());
        let mut trial = String::new();
        for x in iter {
            trial.push(if x > 0 {'1'} else {'0'});
        }
        assert_eq!(trial, format!("{:b}", oracle));
    }

    #[test]
    fn bit_iterator_2() {
        let oracle0 = 0xCCu8;
        let oracle1 = 0x55u8;
        let buf: Vec<u8> = vec!(oracle0, oracle1);
        let iter = BitIterator::new(buf.as_slice());
        let mut trial = String::new();
        for x in iter {
            trial.push(if x > 0 {'1'} else {'0'});
        }
        assert_eq!(trial, format!("{:b}{:08b}", oracle0, oracle1));
    }

    #[test]
    fn huffman_tree_walker_0() {
        let buf = vec!(0xF8u8);
        let iter = BitIterator::new(buf.as_slice());
        let mut walker = HuffmanTreeWalker::new(&*HUFFMAN_TREE);
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
    fn huffman_tree_walker_1() {
        let buf = vec!(0x53u8, 0xF8u8);
        let iter = BitIterator::new(buf.as_slice());
        let mut walker = HuffmanTreeWalker::new(&*HUFFMAN_TREE);
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

    #[test]
    fn encode_0() {
        let buf = vec!(38u8);
        let mut trial: Vec<u8> = vec!();
        encode(&mut trial, buf.as_slice());
        assert_eq!(trial, [0xF8u8]);
    }

    #[test]
    fn encode_1() {
        let buf = vec!(32u8);
        let mut trial: Vec<u8> = vec!();
        encode(&mut trial, buf.as_slice());
        assert_eq!(trial, [0x53u8]);
    }

    #[test]
    fn encode_2() {
        let buf = vec!(32u8, 33u8);
        let mut trial: Vec<u8> = vec!();
        encode(&mut trial, buf.as_slice());
        assert_eq!(trial, [0x53u8, 0xF8u8]);
    }


    fn random_str() -> Vec<u8> {
        const ALPHABET_SIZE: u64 = 256;
        let mut rng = random::default();
        let mut res: Vec<u8> = vec!();
        loop {
            let x = rng.read_u64() % (ALPHABET_SIZE + ALPHABET_SIZE / 10);
            if x >= ALPHABET_SIZE {
                break;
            }
            res.push(x as u8);
        }
        res
    }
   
    #[test]
    fn encode_decode_random() {
        for _ in 0..1000 {
            let oracle = random_str();

            let mut encoded = vec!();
            encode(&mut encoded, oracle.as_slice());

            let trial = decode(encoded.as_slice()).unwrap();
            assert_eq!(trial, oracle);
        }
    }
}
