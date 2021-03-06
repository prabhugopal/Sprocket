extern crate simple_fs as fs;
extern crate slice_cast;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::env;
use std::cell::RefCell;
use std::mem::size_of;

// size in blocks
const FS_SIZE: u32 = 1000;

fn main() {
    let path = env::args().nth(1).expect("You must pass a path!");
    let mut fs = fs::FileSystem::new(DiskFile::new(path));
    match mkfs(&mut fs) {
        Ok(_) => println!("The disk was successfully formatted!"),
        Err(e) => panic!("An {:?} error occurred while formatting", e),
    }
    // for each specified file, copy it into the new file system
    for arg in env::args().skip(2) {
        println!("Writing {}", arg);
        write_file(&mut fs, &arg).unwrap();
    }
}

fn write_file<T>(fs: &mut fs::FileSystem<T>, path: &str) -> Result<(), fs::FsError>
    where T: fs::Disk
{
    let mut f = File::open(path).expect("Could not open file");
    let mut inode = fs::Inode {
        type_: fs::InodeType::File,
        device: fs::ROOT_DEV,
        major: 0,
        minor: 0,
        size: 0,
        blocks: [0; fs::NDIRECT],
    };
    let inum = fs.alloc_inode(fs::ROOT_DEV, inode).unwrap();
    assert_ne!(inum, fs::ROOT_INUM);
    let mut buf = vec![];
    f.read_to_end(&mut buf).unwrap();
    fs.write(&mut inode, &buf, 0).unwrap();
    fs.update_inode(inum, &inode).unwrap();

    let new_inode = fs.read_inode(fs::ROOT_DEV, inum).unwrap();
    let mut buf2 = Vec::with_capacity(new_inode.size as usize);
    fs.read(&new_inode, buf2.as_mut_slice(), 0).unwrap();
    assert_eq!(buf.len(), new_inode.size as usize);
    for (i, b) in buf.iter().enumerate() {
        let mut b2 = [0u8; 1];
        fs.read(&new_inode, &mut b2[0..1], i as u32).unwrap();
        assert_eq!(*b, b2[0]);
    }
    println!("File writeback was successful!");

    let name = path.bytes().take(fs::DIRNAME_SIZE).collect::<Vec<_>>();
    let mut root = fs.read_inode(fs::ROOT_DEV, fs::ROOT_INUM).unwrap();
    fs.dir_add(&mut root, name.as_slice(), inum).unwrap();
    fs.update_inode(fs::ROOT_INUM, &root)?;

    Ok(())
}

fn mkfs<T>(fs: &mut fs::FileSystem<T>) -> Result<(), fs::FsError>
    where T: fs::Disk
{
    // Write an unused inode for each of the inodes
    for i in 0..fs::NUM_INODES {
        fs.update_inode(i, &fs::UNUSED_INODE)?;
    }

    // create the freelist
    let inode_size = size_of::<fs::Inode>();
    let datablocks_start = 1 + (fs::NUM_INODES as u32) / ((fs::BLOCKSIZE / inode_size) as u32);

    let tmp_sb = fs::SuperBlock {
        size: 0,
        nblocks: FS_SIZE,
        ninodes: fs::NUM_INODES,
        inode_start: 1,
        freelist_start: fs::UNUSED_BLOCKADDR,
    };
    let mut buf = [0u8; fs::BLOCKSIZE];
    {
        let sb: &mut fs::SuperBlock =
            unsafe { &mut slice_cast::cast_mut(&mut buf[..size_of::<fs::SuperBlock>()])[0] };

        *sb = tmp_sb;
    }
    // write the new superblock
    fs.disk.write(&buf, 0, fs::SUPERBLOCK_ADDR)?;

    // sanity check: read the superblock back to verify it was written properly
    {
        let sb2: fs::SuperBlock =
            unsafe { *slice_cast::cast_to_mut(&mut buf[..size_of::<fs::SuperBlock>()]) };
        fs.disk.read(&mut buf, 0, fs::SUPERBLOCK_ADDR)?;
        assert_eq!(sb2, tmp_sb);
    }

    // add every data block to the freelist
    for blockno in datablocks_start..FS_SIZE {
        fs.free_block(0, blockno).unwrap();
    }
    println!("Successfully created block free list");


    // finally, create root dir
    let mut inode = fs::Inode {
        type_: fs::InodeType::Directory,
        device: fs::ROOT_DEV,
        major: 0,
        minor: 0,
        size: 0,
        blocks: [0; fs::NDIRECT],
    };

    let dirent_size = size_of::<fs::DirEntry>();

    assert_eq!(fs.alloc_inode(0, inode)?, fs::ROOT_INUM);
    println!("Writing root directory");
    fs.dir_add(&mut inode, b".", fs::ROOT_INUM)?;
    assert_eq!(inode.size as usize, dirent_size);
    fs.dir_add(&mut inode, b"..", fs::ROOT_INUM)?;
    assert_eq!(inode.size as usize, 2 * dirent_size);
    fs.update_inode(fs::ROOT_INUM, &inode)?;

    let inum2 = fs.read_inode(fs::ROOT_DEV, fs::ROOT_INUM)?;
    assert_eq!(inode.type_, inum2.type_);
    assert_eq!(inode.size, inum2.size);
    assert_eq!(inode.blocks[0], inum2.blocks[0]);

    assert_eq!(fs.dir_lookup(&inode, b"."), Ok((0, 0)));
    assert_eq!(fs.dir_lookup(&inode, b".."),
               Ok((0, size_of::<fs::DirEntry>())));
    Ok(())
}

struct DiskFile {
    file: RefCell<File>,
}

impl DiskFile {
    fn new(path: std::string::String) -> DiskFile {
        DiskFile {
            file: RefCell::new(OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .expect("Could not create file")),
        }
    }
}

impl fs::Disk for DiskFile {
    fn read(&self, mut buffer: &mut [u8], _: u32, sector: u32) -> Result<(), fs::DiskError> {
        // seek to the sector
        assert!(buffer.len() <= 512);
        let _ = self.file
            .borrow_mut()
            .seek(SeekFrom::Start((sector as u64) * (Self::sector_size()) as u64));
        let _ = self.file.borrow_mut().read_exact(&mut buffer);
        Ok(())
    }

    fn write(&mut self, buffer: &[u8], _: u32, sector: u32) -> Result<usize, fs::DiskError> {
        assert!(buffer.len() <= 512, "length was {}", buffer.len());
        let _ = self.file
            .borrow_mut()
            .seek(SeekFrom::Start((sector as u64) * (Self::sector_size()) as u64));
        let written = self.file.borrow_mut().write_all(buffer);
        self.file.borrow_mut().flush().unwrap();
        if written.is_ok() {
            Ok(buffer.len())
        } else {
            Err(fs::DiskError::IoError)
        }
    }

    fn sector_size() -> usize {
        512
    }
}
