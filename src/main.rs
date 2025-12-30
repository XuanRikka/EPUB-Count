use std::fs::{OpenOptions};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::thread::{available_parallelism, JoinHandle};

use clap::Parser;
use zip::ZipArchive;
use scraper::Html;
use walkdir::{DirEntry, WalkDir};
use memmap2::Mmap;

/// 一个用于统计 EPUB 文件字数的小工具
///
/// 支持直接指定文件，或通过 `-w` 递归遍历目录。
#[derive(Parser)]
#[command(
    version,
    about,
    long_about = None,
)]
struct Cli
{
    /// 要统计的 EPUB 文件路径（支持多个）
    ///
    /// 可传入 `.epub` 文件，或配合 `-w` 传入目录。
    #[arg(required = true)]
    files: Vec<String>,

    /// 递归遍历目录（walk directories）
    ///
    /// 当传入的是目录时，自动查找其中所有 `.epub` 文件并统计。
    #[arg(short ,long, default_value_t = false, action = clap::ArgAction::SetTrue)]
    walk: bool,

    /// 调整使用的线程数，默认为cpu线程数
    #[arg(short, long, default_value_t = get_cpu_count())]
    cpu_nums: usize
}


fn get_cpu_count() -> usize {
    available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}


trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}


struct FileData
{
    filename: String,
    file: PathBuf
}

struct FileWordCount
{
    filename: String,
    word_count: u64
}


pub fn get_all_epub_walkdir<P: AsRef<Path>>(path: P) -> Vec<PathBuf> {
    fn is_epub(entry: &DirEntry) -> bool {
        entry.file_type().is_file()
            && entry
            .path()
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("epub"))
    }

    WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| {
            e.file_type().is_dir() || is_epub(e)
        })
        .filter_map(|e| e.ok())
        .filter(is_epub)
        .map(|e| e.into_path())
        .collect()
}


fn html_word_count(string: &String) -> u64
{
    Html::parse_document(string)
        .root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .chars()
        .count() as u64
}


fn zip_xhtml_read<W: Read + Seek>(file: W) -> Vec<String> {
    let mut zip = ZipArchive::new(file).expect("读取zip文件时出现错误");

    let n = zip.len();
    let mut results = Vec::new();

    for i in 0..n {
        let mut file = zip.by_index(i).expect("遍历zip文件列表时出现错误");
        let name = file.name();

        if !(name.ends_with(".xhtml") || name.ends_with(".html")) {
            continue;
        }
        if name == "toc.xhtml" || name == "toc.html" {
            continue;
        }

        let size = file.size();
        let mut content = String::with_capacity(size as usize);

        file.read_to_string(&mut content).expect("读取xhtml文件时出现错误");
        results.push(content);
    }

    results
}

fn get_epub_word_count<P: AsRef<Path>>(path: P) -> u64
{
    let file = open_file(path);
    let chars = zip_xhtml_read(file);
    let word_count: u64 = chars.iter().map(
        |s| html_word_count(s)
    ).sum::<u64>();

    word_count
}


fn split_vec<T>(mut vec: Vec<T>, n: usize) -> Vec<Vec<T>> {
    if n == 0 || vec.is_empty() {
        return vec![vec];
    }

    let len = vec.len();
    let chunk_size = (len + n - 1) / n;
    let mut result = Vec::new();

    while !vec.is_empty() {
        let take = chunk_size.min(vec.len());
        let chunk = vec.drain(..take).collect::<Vec<T>>();
        result.push(chunk);
    }

    result
}


fn open_file<P: AsRef<Path>>(p: P) -> Box<dyn ReadSeek>
{
    let file = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(p)
        .expect("打开文件失败");
    let file_mmap = unsafe { Mmap::map(&file) };
    match file_mmap {
        Ok(mmap) => Box::new(Cursor::new(mmap)),
        Err(e) => {
            Box::new(file)
        }
    }
}


fn main()
{
    let args = Cli::parse();

    let mut epub_renders: Vec<FileData> = Vec::new();

    for file in &args.files {
        let path = PathBuf::from(file.as_str());

        if !path.exists() {
            eprintln!("文件/目录 {} 不存在", file);
            continue;
        }

        if args.walk && path.is_dir()
        {
            for p in get_all_epub_walkdir(path.clone()) {
                let s = FileData {
                    filename: p.file_name().unwrap().to_str().unwrap().to_string(),
                    file: p
                };
                epub_renders.push(s);
            }
        }
        else if !args.walk && path.is_dir()
        {
            continue;
        }
        else if path.is_file()
        {
            let s = FileData {
                filename: path.file_name().unwrap().to_str().unwrap().to_string(),
                file: path
            };
            epub_renders.push(s);
        }
        else
        {
            panic!("未知输入")
        }
    }

    if epub_renders.is_empty()
    {
        eprintln!("没有找到任何EPUB文件");
        exit(0)
    }



    let mut total_word_count: u64 = 0;
    let mut threads: Vec<JoinHandle<Vec<FileWordCount>>> = Vec::new();
    for files in split_vec(epub_renders, args.cpu_nums)
    {
        threads.push(thread::spawn(move || {
            let mut infos: Vec<FileWordCount> = Vec::new();
            for f in files
            {
                let word_count = get_epub_word_count(f.file);
                let info = FileWordCount{
                    filename: f.filename,
                    word_count
                };
                infos.push(info);
            }
            infos
        }))
    }

    for handle in threads {
        let infos = handle.join().unwrap();
        for info in infos {
            println!("{} 字数：{} 字", info.filename, info.word_count);
            total_word_count += info.word_count;
        }
    }

    println!("总字数：{} 字", total_word_count)
}
