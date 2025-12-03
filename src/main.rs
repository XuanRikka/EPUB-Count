use clap::Parser;

use std::fs::File;
use std::io::{BufReader, Read};
use zip::ZipArchive;
use std::path::PathBuf;
use scraper::Html;

#[derive(Parser)]
struct Cli
{
    #[arg(required = true)]
    files: Vec<String>,
}


fn html_word_count(string: &String) -> usize
{
    Html::parse_document(string)
        .root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .chars()
        .count()
}


fn zip_xhtml_read(file: &File) -> Vec<String>
{
    let buf = BufReader::new(file);
    let mut zip = ZipArchive::new(buf).expect("打开EPUB文件打开失败");
    let text_paths: Vec<String> = zip.file_names()
        .filter(|x| x.ends_with(".xhtml")&&(x.starts_with("OEBPS/Text/")||x.starts_with("EPUB/Text/")))
        .map(|s| s.to_string()).collect();
    return text_paths
        .into_iter().map(|path| {
            let mut file = zip.by_name(path.as_str()).unwrap();
            let mut s = String::new();
            file.read_to_string(&mut s).unwrap();
            s
        })
        .collect::<Vec<String>>();
}


fn main() {
    let args = Cli::parse();

    let mut paths: Vec<PathBuf> = Vec::new();
    for file in args.files
    {
        let path = PathBuf::from(file.as_str());
        if path.exists()
        {
            paths.push(path);
        }
        else
        {
            eprintln!("文件 {} 不存在", file);
        }
    }

    let files = paths.iter()
        .map(|path| {
            File::open(path).expect("打开文件失败")
        })
        .collect::<Vec<File>>();

    let xhtml_texts = files.iter()
        .map(|file| {
            zip_xhtml_read(file)
        })
        .flatten()
        .collect::<Vec<String>>();

    let word_count = xhtml_texts.iter()
        .map(|text| {
            html_word_count(text)
        })
        .sum::<usize>();

    println!("总字数：{}", word_count);
}
