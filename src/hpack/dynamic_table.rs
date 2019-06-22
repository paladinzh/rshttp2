use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Debug, Formatter, Error};
use std::hash::Hasher;
use std::marker::{PhantomPinned, PhantomData};
use std::mem::swap;
use std::ops::Bound::{Included, Unbounded};
use std::pin::Pin;
use std::ptr;
use std::slice;
use std::sync::Arc;
use super::super::Sliceable;

pub struct DynamicTable {
    h2_used_size: usize,
    h2_limit_size: usize,
    seq_id_gen: SeqIdGen,
    cache: Cache,
    seq_id_range: Option<(SeqId, SeqId)>,
}

unsafe impl Send for DynamicTable {}

impl DynamicTable {
    pub fn with_capacity(cap: usize) -> DynamicTable {
        DynamicTable{
            h2_used_size: 0,
            h2_limit_size: cap,
            seq_id_gen: SeqIdGen::new(),
            cache: Cache::new(cap),
            seq_id_range: None,
        }
    }

    pub fn get(&self, index: usize) -> Option<Item> {
        assert!(self.seq_id_range.is_some());
        let (start, end) = self.seq_id_range.unwrap();
        let index = index as u64;
        if start + index > end {
            return None;
        }
        let seq_id = end - index;
        let (block, item) = self.cache.get(seq_id).unwrap();
        let res = Item{
            name: CachedStr::new(block.clone(), item.name, item.name_len),
            value: Some(CachedStr::new(block, item.value, item.value_len)),
            index: index as usize,
        };
        Some(res)
    }

    pub fn prepend(&mut self, name: &[u8], value: &[u8]) -> Option<Item> {
        let size = h2_size(name, value);
        let room = self.make_room(size);
        match room {
            MakeRoomResult::NoRoom => None,
            MakeRoomResult::Enough => {
                self.h2_used_size += size;
                let seq_id = self.seq_id_gen.next();
                self.seq_id_range = match self.seq_id_range {
                    None => Some((seq_id, seq_id)),
                    Some((s, _)) => Some((s, seq_id)),
                };
                let (block, item) = self.cache.append(seq_id, name, value);
                let res = Item{
                    name: CachedStr::new(block.clone(), item.name, item.name_len),
                    value: Some(CachedStr::new(block, item.value, item.value_len)),
                    index: 0,
                };
                Some(res)
            }
        }
    }

    pub fn len(&self) -> usize {
        match self.seq_id_range {
            None => 0,
            Some((s, e)) => (e - s + 1) as usize,
        }
    }

    pub fn update_capacity(&mut self, new_cap: usize) -> () {
        self.cache.update_block_size(new_cap);
        self.h2_limit_size = new_cap;
        self.make_room(0);
    }

    pub fn seek_with_name(&self, name: &[u8]) -> Option<usize> {
        match self.cache.seek_with_name(name) {
            None => None,
            Some(seq_id) => {
                let idx = self.seq_id_to_index(seq_id);
                assert!(idx.is_some());
                idx
            }
        }
    }

    pub fn seek_with_name_value(&self, name: &[u8], value: &[u8]) -> Option<usize> {
        match self.cache.seek_with_name_value(name, value) {
            None => None,
            Some(seq_id) => {
                let idx = self.seq_id_to_index(seq_id);
                assert!(idx.is_some());
                idx
            }
        }
    }

    fn seq_id_to_index(&self, seq_id: SeqId) -> Option<usize> {
        match self.seq_id_range {
            None => None,
            Some((s, e)) => {
                assert!(s <= seq_id);
                assert!(seq_id <= e);
                Some((e - seq_id) as usize)
            }
        }
    }
    
    fn make_room(&mut self, space: usize) -> MakeRoomResult {
        let (start_id, end_id) = match self.seq_id_range {
            None => {
                assert_eq!(self.h2_used_size, 0);
                if space < self.h2_limit_size {
                    return MakeRoomResult::Enough;
                } else {
                    return MakeRoomResult::NoRoom;
                }
            },
            Some(x) => x
        };
        let mut new_start_id = start_id;
        while self.h2_used_size + space > self.h2_limit_size && start_id <= end_id {
            let (_, cached) = self.cache.get(new_start_id).unwrap();
            let size = h2_size_from_len(cached.name_len, cached.value_len);
            assert!(size <= self.h2_used_size);
            self.h2_used_size -= size;
            new_start_id += 1;
        }
        if new_start_id > end_id {
            self.seq_id_range = None;
            self.cache.truncate(end_id);
        } else {
            self.cache.truncate(new_start_id);
            self.seq_id_range = Some((new_start_id, end_id));
        }
        if self.h2_used_size + space <= self.h2_limit_size {
            MakeRoomResult::Enough
        } else {
            MakeRoomResult::NoRoom
        }
    }
}

fn h2_size(name: &[u8], value: &[u8]) -> usize {
    h2_size_from_len(name.len(), value.len())
}

fn h2_size_from_len(name_len: usize, value_len: usize) -> usize {
    name_len + value_len + 32
}

#[derive(Debug)]
enum MakeRoomResult {
    NoRoom,
    Enough,
}

#[derive(Debug)]
pub struct Item {
    pub name: CachedStr,
    pub value: Option<CachedStr>,
    pub index: usize,
}

#[derive(Clone)]
pub struct CachedStr {
    block: PinnedCacheBlock,
    ptr: *const u8,
    len: usize,
}

impl CachedStr {
    fn new(block: PinnedCacheBlock, ptr: *const u8, len: usize) -> CachedStr {
        CachedStr{
            block,
            ptr,
            len,
        }
    }
}

impl Sliceable<u8> for CachedStr {
    fn as_slice(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(self.ptr, self.len)
        }
    }
}

impl Debug for CachedStr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.as_slice().fmt(f)
    }
}


type SeqId = u64;

struct SeqIdGen {
    last: SeqId,
}

impl SeqIdGen {
    fn new() -> SeqIdGen {
        SeqIdGen{
            last: 0,
        }
    }

    fn next(&mut self) -> SeqId {
        let res = self.last;
        self.last += 1;
        assert!(self.last & (1 << 63) == 0);
        res
    }
}

type PinnedCacheBlock = Pin<Arc<RefCell<CacheBlock>>>;

struct Cache {
    first_block: PinnedCacheBlock,
    last_block: PinnedCacheBlock,
    size_for_next_block: usize,
}

impl Cache {
    fn new(block_size: usize) -> Cache {
        let block = CacheBlock::new(block_size);
        Cache{
            first_block: block.clone(),
            last_block: block,
            size_for_next_block: block_size,
        }
    }

    fn append(
        &mut self,
        seq_id: SeqId,
        name: &[u8],
        value: &[u8],
    ) -> (PinnedCacheBlock, CacheItem) {
        let last_block = mutref_cache_block_from_pinned(&self.last_block);
        match last_block.append(seq_id, name, value) {
            Some(x) => (self.last_block.clone(), x),
            None => {
                let new_block = CacheBlock::new(self.size_for_next_block);
                last_block.set_next_block(new_block.clone());
                self.last_block = new_block.clone();
                let x = {
                    let new_block = mutref_cache_block_from_pinned(&new_block);
                    let x = new_block.append(seq_id, name, value);
                    assert!(x.is_some());
                    x.unwrap()
                };
                (new_block, x)
            }
        }
    }

    fn get(&self, seq_id: SeqId) -> Option<(PinnedCacheBlock, CacheItem)> {
        for block in self.iter() {
            let ref_blk = ref_cache_block_from_pinned(&block);
            match ref_blk.get_last_seq_id() {
                None => {
                    return None;
                },
                Some(last_seq_id) => {
                    if seq_id <= last_seq_id {
                        match ref_blk.get(seq_id) {
                            None => {
                                return None;
                            },
                            Some(x) => {
                                return Some((block.clone(), x));
                            }
                        }
                    }
                }
            };
        }
        unreachable!();
    }

    fn seek_with_name(&self, name: &[u8]) -> Option<SeqId> {
        let name_digest = digest_name(name);
        for block in self.iter() {
            let block = ref_cache_block_from_pinned(&block);
            match block.seek_with_name(name_digest, name) {
                Some(ref item) => {
                    return Some(item.seq_id);
                },
                None => (),
            };
        };
        None
    }

    fn seek_with_name_value(&self, name: &[u8], value: &[u8]) -> Option<SeqId> {
        let (name_digest, name_value_digest) = digest_name_value(name, value);
        for block in self.iter() {
            let block = ref_cache_block_from_pinned(&block);
            match block.seek_with_name_value(
                name_digest, name,
                name_value_digest, value) {
                Some(ref item) => {
                    return Some(item.seq_id);
                },
                None => (),
            };
        }
        None
    }

    fn truncate(&mut self, seq_id: SeqId) -> () {
        loop {
            let nxt = {
                let blk = mutref_cache_block_from_pinned(&self.first_block);
                match blk.get_last_seq_id() {
                    None => {
                        return;
                    },
                    Some(last_seq_id) => {
                        if last_seq_id >= seq_id {
                            return;
                        }
                    }
                };
                if blk.next_block.is_none() {
                    return;
                }
                let nxt = blk.next_block.take();
                nxt.unwrap()
            };
            self.first_block = nxt;
        }
    }

    fn update_block_size(&mut self, new_size: usize) -> () {
        self.size_for_next_block = new_size;
    }

    fn iter<'a>(&'a self) -> CacheBlockIter<'a> {
        CacheBlockIter::<'a>::new(self.first_block.clone())
    }
}

struct CacheBlockIter<'a> {
    nxt_block: Option<PinnedCacheBlock>,
    _phantom: PhantomData<&'a PinnedCacheBlock>,
}

impl<'a> CacheBlockIter<'a> {
    fn new(first: PinnedCacheBlock) -> CacheBlockIter<'a> {
        CacheBlockIter{
            nxt_block: Some(first),
            _phantom: PhantomData,
        }
    }
}

impl<'a> Iterator for CacheBlockIter<'a> {
    type Item = PinnedCacheBlock;

    fn next(&mut self) -> Option<PinnedCacheBlock> {
        if self.nxt_block.is_none() {
            None
        } else {
            let mut cur_block = None;
            swap(&mut cur_block, &mut self.nxt_block);
            {
                let blk = cur_block.as_ref();
                let blk = blk.unwrap();
                let blk = ref_cache_block_from_pinned(blk);
                self.nxt_block = blk.next_block.clone();
            }
            cur_block
        }
    }
}

struct CacheBlock {
    _pin: PhantomPinned,
    next_block: Option<PinnedCacheBlock>,
    
    buffer: Vec<u8>,
    end_of_buffer: *const u8,
    begin_of_unused: *mut u8,
    index_on_seq_id: BTreeMap<SeqId, CacheItem>,
    last_seq_id: Option<SeqId>,
    index_on_name_value: BTreeSet<CacheItem>,
}

impl CacheBlock {
    fn new(block_size: usize) -> PinnedCacheBlock {
        let res = Arc::pin(RefCell::new(CacheBlock{
            _pin: PhantomPinned,
            next_block: None,
            buffer: vec!(),
            end_of_buffer: ptr::null(),
            begin_of_unused: ptr::null_mut(),
            index_on_seq_id: BTreeMap::new(),
            last_seq_id: None,
            index_on_name_value: BTreeSet::new(),
        }));
        {
            let res = mutref_cache_block_from_pinned(&res);
            res.buffer.resize(block_size, 0);
            res.begin_of_unused = res.buffer.as_mut_ptr();
            res.end_of_buffer = unsafe {
                res.begin_of_unused.add(res.buffer.len())
            };
        }
        res
    }

    fn append(
        &mut self,
        seq_id: SeqId,
        name: &[u8],
        value: &[u8],
    ) -> Option<CacheItem> {
        assert!(self.last_seq_id.is_none() || seq_id == self.last_seq_id.unwrap() + 1);
        let (name_digest, name_value_digest) = digest_name_value(name, value);
        unsafe {
            let begin_of_name = self.begin_of_unused;
            let begin_of_value = begin_of_name.add(name.len());
            let end_of_value = begin_of_value.add(value.len());
            if end_of_value as *const u8 > self.end_of_buffer {
                return None;
            }
            let item = CacheItem{
                seq_id,

                name: begin_of_name,
                name_len: name.len(),
                name_digest,

                value: begin_of_value,
                value_len: value.len(),
                name_value_digest,
            };
            ptr::copy_nonoverlapping(name.as_ptr(), begin_of_name, name.len());
            ptr::copy_nonoverlapping(value.as_ptr(), begin_of_value, value.len());
            self.begin_of_unused = end_of_value;
            self.index_on_seq_id.insert(seq_id, item.clone());
            self.index_on_name_value.insert(item.clone());
            self.last_seq_id = Some(seq_id);
            Some(item)
        }
    }

    fn get(&self, seq_id: SeqId) -> Option<CacheItem> {
        match self.index_on_seq_id.get(&seq_id) {
            None => None,
            Some(x) => Some(x.clone()),
        }
    }

    fn seek_with_name(&self, name_digest: u64, name: &[u8]) -> Option<&CacheItem> {
        const MIN_VALUE: &[u8] = b"";
        let lower_bound = CacheItem{
            seq_id: 0,
            name: name.as_ptr(),
            name_len: name.len(),
            name_digest,
            value: MIN_VALUE.as_ptr(),
            value_len: 0,
            name_value_digest: 0,
        };
        for item in self.index_on_name_value.range((Included(&lower_bound), Unbounded)) {
            if item.name_digest > name_digest {
                return None;
            }
            if item.name_len > name.len() {
                return None;
            }
            let item_name = unsafe {
                slice::from_raw_parts(item.name, item.name_len)
            };
            if item_name > name {
                return None;
            }
            return Some(item);
        }
        None
    }

    fn seek_with_name_value(
        &self,
        name_digest: u64,
        name: &[u8],
        name_value_digest: u64,
        value: &[u8],
    ) -> Option<&CacheItem> {
        let lower_bound = CacheItem{
            seq_id: 0,
            name: name.as_ptr(),
            name_len: name.len(),
            name_digest,
            value: value.as_ptr(),
            value_len: value.len(),
            name_value_digest,
        };
        for item in self.index_on_name_value.range((Included(&lower_bound), Unbounded)) {
            if item.name_digest > name_digest {
                return None;
            }
            if item.name_len > name.len() {
                return None;
            }
            let item_name = unsafe {
                slice::from_raw_parts(item.name, item.name_len)
            };
            if item_name > name {
                return None;
            }
            if item.name_value_digest > name_value_digest {
                return None;
            }
            if item.value_len > value.len() {
                return None;
            }
            let item_value = unsafe {
                slice::from_raw_parts(item.value, item.value_len)
            };
            if item_value > value {
                return None;
            }
            return Some(item);
        }
        None
    }

    fn get_last_seq_id(&self) -> Option<SeqId> {
        self.last_seq_id
    }

    fn set_next_block(&mut self, next_block: PinnedCacheBlock) -> () {
        assert!(self.next_block.is_none());
        self.next_block = Some(next_block);
    }
}

impl Debug for CacheBlock {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_fmt(format_args!(
            "CacheBlock@{:p}(next: {:?}, buffer: {}, used: {}, items: {}, last_seq_id: {:?})",
            self as *const CacheBlock,
            match self.next_block {
                None => None,
                Some(ref x) => {
                    let x = x.as_ref();
                    let x = x.get_ref();
                    let x = x.borrow();
                    let x = &*x;
                    let x = x as *const CacheBlock;
                    Some(format!("{:p}", x))
                }
            },
            self.buffer.len(),
            (self.begin_of_unused as usize) - (self.buffer.as_ptr() as usize),
            self.index_on_name_value.len(),
            self.last_seq_id,
        ))
    }
}

#[derive(Debug, Clone)]
struct CacheItem {
    seq_id: SeqId,

    name: *const u8,
    name_len: usize,
    name_digest: u64,

    value: *const u8,
    value_len: usize,
    name_value_digest: u64,
}

macro_rules! try_cmp {
    ($e0: expr, $e1: expr) => {
        let ord = $e0.cmp(&$e1);
        if ord != Ordering::Equal {
            return ord;
        }
    }
}

impl Ord for CacheItem {
    fn cmp(&self, other: &Self) -> Ordering {
        try_cmp!(self.name_digest, other.name_digest);
        try_cmp!(self.name_len, other.name_len);
        {
            let self_name = get_cached_name(self, self);
            let other_name = get_cached_name(other, other);
            try_cmp!(self_name, other_name);
        }
        try_cmp!(self.name_value_digest, other.name_value_digest);
        try_cmp!(self.value_len, other.value_len);
        {
            let self_value = get_cached_value(self, self);
            let other_value = get_cached_value(other, other);
            try_cmp!(self_value, other_value);
        }
        try_cmp!(self.seq_id, other.seq_id);
        Ordering::Equal
    }
}

impl PartialOrd for CacheItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CacheItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for CacheItem {}

fn digest_name(name: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(name);
    hasher.finish()
}

fn digest_name_value(name: &[u8], value: &[u8]) -> (u64, u64) {
    let mut hasher = DefaultHasher::new();
    hasher.write(name);
    let name_digest = hasher.finish();
    hasher.write(value);
    let name_value_digest = hasher.finish();
    (name_digest, name_value_digest)
}

fn get_cached_name<'a, 'b, T>(_: &'a T, cached: &'b CacheItem) -> &'a [u8] {
    unsafe {
        slice::from_raw_parts(cached.name, cached.name_len)
    }
}

fn get_cached_value<'a, 'b, T>(_: &'a T, cached: &'b CacheItem) -> &'a [u8] {
    unsafe {
        slice::from_raw_parts(cached.value, cached.value_len)
    }
}

fn ref_cache_block_from_pinned(pinned: &PinnedCacheBlock) -> &CacheBlock {
    let res = pinned.as_ref();
    let res = res.get_ref().borrow();
    let res = &*res;
    let res = res as *const CacheBlock;
    unsafe {
        &*res
    }
}

fn mutref_cache_block_from_pinned(pinned: &PinnedCacheBlock) -> &mut CacheBlock {
    let res = pinned.as_ref();
    let mut res = res.get_ref().borrow_mut();
    let res = &mut *res;
    let res = res as *mut CacheBlock;
    unsafe {
        &mut *res
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn chopping() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        const KEY2: &[u8] = b"hello2";
        const VALUE2: &[u8] = b"world2";
        // large enough to hold 2 KEY-VALUEs, but less than 3 of them.
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        dyntbl.prepend(KEY1, VALUE1);
        dyntbl.prepend(KEY2, VALUE2);
        assert_eq!(dyntbl.len(), 2);
        assert_eq!(dyntbl.get(0).unwrap().name.as_slice(), KEY2);
        assert_eq!(dyntbl.get(0).unwrap().value.unwrap().as_slice(), VALUE2);
        assert_eq!(dyntbl.get(1).unwrap().name.as_slice(), KEY1);
        assert_eq!(dyntbl.get(1).unwrap().value.unwrap().as_slice(), VALUE1);
        assert!(dyntbl.get(2).is_none());
    }

    #[test]
    fn update_capacity() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        dyntbl.update_capacity(0);
        dyntbl.update_capacity(100);
        assert_eq!(dyntbl.len(), 0);
        
        dyntbl.prepend(KEY1, VALUE1);
        assert_eq!(dyntbl.len(), 1);
        assert_eq!(dyntbl.get(0).unwrap().name.as_slice(), KEY1);
        assert_eq!(dyntbl.get(0).unwrap().value.unwrap().as_slice(), VALUE1);
    }

    #[test]
    fn seek_no_hit() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        assert!(dyntbl.seek_with_name_value(KEY1, VALUE1).is_none());
        assert!(dyntbl.seek_with_name(KEY1).is_none());
    }

    #[test]
    fn seek_hit_key() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const VALUE1: &[u8] = b"world1";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        assert!(dyntbl.seek_with_name(KEY0).is_some());
        assert_eq!(dyntbl.seek_with_name(KEY0).unwrap(), 0);
        assert!(dyntbl.seek_with_name_value(KEY0, VALUE1).is_none());
    }

    #[test]
    fn seek_hit_both() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        let seeked = dyntbl.seek_with_name_value(KEY0, VALUE0);
        assert!(seeked.is_some());
        assert_eq!(seeked.unwrap(), 0);
    }

    #[test]
    fn cacheblock_insert_and_get() {
        const BLOCK_SIZE: usize = 15; // large enough to hold a key-value.
        let cb = CacheBlock::new(BLOCK_SIZE);
        let cb: &mut CacheBlock = mutref_cache_block_from_pinned(&cb);
        let _ = cb.append(1, b"hello", b"world").unwrap();
        let trial = cb.get(1).unwrap();
        assert_eq!(get_cached_name(&cb, &trial), b"hello");
        assert_eq!(get_cached_value(&cb, &trial), b"world");
        assert_eq!(cb.get_last_seq_id(), Some(1));
    }

    #[test]
    fn cacheblock_insert_too_large() {
        const BLOCK_SIZE: usize = 9; // small than a key-value
        let cb = CacheBlock::new(BLOCK_SIZE);
        let cb: &mut CacheBlock = mutref_cache_block_from_pinned(&cb);
        let trial = cb.append(1, b"hello", b"world");
        assert!(trial.is_none());
        assert!(cb.get_last_seq_id().is_none());
    }

    #[test]
    fn cache_insert_in_1st_block() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(1, b"hello", b"world");
        let (_holder, i1) = trial.get(1).unwrap();
        assert_eq!(get_cached_name(&trial, &i1), b"hello");
        assert_eq!(get_cached_value(&trial, &i1), b"world");
    }

    #[test]
    fn cache_insert_new_block() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(1, KEY0, VALUE0);
        trial.append(2, KEY1, VALUE1);
        let (_holder, i1) = trial.get(1).unwrap();
        assert_eq!(get_cached_name(&trial, &i1), KEY0);
        assert_eq!(get_cached_value(&trial, &i1), VALUE0);
        let (_holder, i2) = trial.get(2).unwrap();
        assert_eq!(get_cached_name(&trial, &i2), KEY1);
        assert_eq!(get_cached_value(&trial, &i2), VALUE1);
    }

    #[test]
    fn cache_truncate_0() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        const KEY2: &[u8] = b"hello2";
        const VALUE2: &[u8] = b"world2";
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(0, KEY0, VALUE0);
        trial.append(1, KEY1, VALUE1);
        trial.append(2, KEY2, VALUE2);
        trial.truncate(2);
        let i0 = trial.get(0);
        assert!(i0.is_none());
        let i1 = trial.get(1);
        assert!(i1.is_none());
        let (_holder, i2) = trial.get(2).unwrap();
        assert_eq!(get_cached_name(&trial, &i2), KEY2);
        assert_eq!(get_cached_value(&trial, &i2), VALUE2);
    }

    #[test]
    fn cache_truncate_1() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(0, KEY0, VALUE0);
        trial.truncate(0);
        trial.append(1, KEY1, VALUE1);
        let (_holder, i1) = trial.get(1).unwrap();
        assert_eq!(get_cached_name(&trial, &i1), KEY1);
        assert_eq!(get_cached_value(&trial, &i1), VALUE1);
    }

    #[test]
    fn cacheblockiterator_1() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(0, KEY0, VALUE0);
        let mut iter = trial.iter();
        {
            let v = iter.next();
            assert!(v.is_some());
            let v = v.unwrap();
            let v = ref_cache_block_from_pinned(&v);
            let v = v.get(0);
            assert!(v.is_some());
            let v = v.unwrap();
            assert_eq!(get_cached_name(&trial, &v), KEY0);
            assert_eq!(get_cached_value(&trial, &v), VALUE0);
        }
        {
            let v = iter.next();
            assert!(v.is_none());
        }
    }

    #[test]
    fn cacheblockiterator_2() {
        // large enough to hold a pair of key-value, 
        // but not large enough to hold two of them.
        const BLOCK_SIZE: usize = 15;
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        let mut trial = Cache::new(BLOCK_SIZE);
        trial.append(0, KEY0, VALUE0);
        trial.append(1, KEY1, VALUE1);
        let mut iter = trial.iter();
        {
            let v = iter.next();
            assert!(v.is_some());
            let v = v.unwrap();
            let v = ref_cache_block_from_pinned(&v);
            let v = v.get(0);
            assert!(v.is_some());
            let v = v.unwrap();
            assert_eq!(get_cached_name(&trial, &v), KEY0);
            assert_eq!(get_cached_value(&trial, &v), VALUE0);
        }
        {
            let v = iter.next();
            assert!(v.is_some());
            let v = v.unwrap();
            let v = ref_cache_block_from_pinned(&v);
            let v = v.get(1);
            assert!(v.is_some());
            let v = v.unwrap();
            assert_eq!(get_cached_name(&trial, &v), KEY1);
            assert_eq!(get_cached_value(&trial, &v), VALUE1);
        }
        {
            let v = iter.next();
            assert!(v.is_none());
        }
    }
}

