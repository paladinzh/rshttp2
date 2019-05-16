
use std::collections::VecDeque;

pub struct DynamicTable {
    items: VecDeque<Item>,
    total_size: usize,
    limit_size: usize,
}

impl DynamicTable {
    pub fn with_capacity(cap: usize) -> DynamicTable {
        DynamicTable{
            items: VecDeque::<Item>::new(),
            total_size: 0,
            limit_size: cap,
        }
    }

    pub fn get(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }

    pub fn prepend(&mut self, name: &[u8], value: &[u8]) -> () {
        let item = Item::new(name, value);
        let r = self.make_room(item.size);
        match r {
            MakeRoomResult::ENOUGH_SPACE => {
                self.total_size += item.size;
                self.items.push_front(item);
            },
            MakeRoomResult::NO_SPACE => ()
        };
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn update_capacity(&mut self, new_cap: usize) -> () {
        self.limit_size = new_cap;
        self.make_room(0);
    }

    pub fn seek<'a, 'b>(&'a self, name: &'b [u8], value: &'b [u8]) -> Option<SeekedItem<'a>> {
        for i in 0..self.len() {
            if name == self.items[i].name.as_slice() && value == self.items[i].value.as_slice() {
                return Some(SeekedItem{
                    index: i,
                    name: self.items[i].name.as_slice(),
                    value: Some(self.items[i].value.as_slice()),
                });
            }
        }
        for i in 0..self.len() {
            if name == self.items[i].name.as_slice() {
                return Some(SeekedItem{
                    index: i,
                    name: self.items[i].name.as_slice(),
                    value: None,
                });
            }
        }
        None
    }

    fn make_room(&mut self, space: usize) -> MakeRoomResult {
        while self.total_size + space > self.limit_size {
            let back_size = match self.items.pop_back() {
                Some(ref x) => x.size,
                None => {
                    break;
                }
            };
            self.total_size -= back_size;
        }
        if self.total_size + space <= self.limit_size {
            MakeRoomResult::ENOUGH_SPACE
        } else {
            MakeRoomResult::NO_SPACE
        }
    }
}

#[derive(Debug)]
enum MakeRoomResult {
    NO_SPACE,
    ENOUGH_SPACE,
}

#[derive(Debug)]
pub struct Item {
    size: usize,
    name: Vec<u8>,
    value: Vec<u8>,
}

impl Item {
    fn new(name: &[u8], value: &[u8]) -> Item {
        const SIZE_OVERHEAD: usize = 32; // defined in RFC-7541(Sec 4.1)
        Item{
            size: name.len() + value.len() + SIZE_OVERHEAD,
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }

    pub fn name(&self) -> &[u8] {
        self.name.as_slice()
    }

    pub fn value(&self) -> &[u8] {
        self.value.as_slice()
    }
}

#[derive(Debug)]
pub struct SeekedItem<'a> {
    index: usize,
    name: &'a [u8],
    value: Option<&'a [u8]>,
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_chopping() {
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
        assert_eq!(dyntbl.get(0).unwrap().name, KEY2);
        assert_eq!(dyntbl.get(0).unwrap().value, VALUE2);
        assert_eq!(dyntbl.get(1).unwrap().name, KEY1);
        assert_eq!(dyntbl.get(1).unwrap().value, VALUE1);
        assert!(dyntbl.get(2).is_none());
    }

    #[test]
    fn test_update_capacity() {
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
        assert_eq!(dyntbl.get(0).unwrap().name, KEY1);
        assert_eq!(dyntbl.get(0).unwrap().value, VALUE1);
    }

    #[test]
    fn test_seek_no_hit() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        let seeked = dyntbl.seek(KEY1, VALUE1);
        assert!(seeked.is_none());
    }

    #[test]
    fn test_seek_hit_key() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        const KEY1: &[u8] = b"hello1";
        const VALUE1: &[u8] = b"world1";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        let seeked = dyntbl.seek(KEY0, VALUE1);
        assert!(seeked.is_some());
        let seeked = seeked.unwrap();
        assert_eq!(seeked.index, 0);
        assert_eq!(seeked.name, KEY0);
        assert!(seeked.value.is_none());
    }

    #[test]
    fn test_seek_hit_both() {
        const KEY0: &[u8] = b"hello0";
        const VALUE0: &[u8] = b"world0";
        // large enough to hold 1 KEY-VALUE
        let mut dyntbl = DynamicTable::with_capacity(100); 
        dyntbl.prepend(KEY0, VALUE0);
        let seeked = dyntbl.seek(KEY0, VALUE0);
        assert!(seeked.is_some());
        let seeked = seeked.unwrap();
        assert_eq!(seeked.index, 0);
        assert_eq!(seeked.name, KEY0);
        assert!(seeked.value.is_some());
        assert_eq!(seeked.value.unwrap(), VALUE0);
    }
}

