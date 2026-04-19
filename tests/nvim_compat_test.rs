use std::process::Command;
use std::fs;

/// Run a comparison between rneovim and nvim
fn assert_nvim_compat(initial_content: &str, keys: &str, test_name: &str) {
    let filename = format!("temp_test_{}.txt", test_name);
    let actual_file = format!("actual_{}.txt", test_name);
    let expected_file = format!("expected_{}.txt", test_name);

    // Initial setup
    fs::write(&filename, initial_content).expect("Failed to write initial file");

    // 1. Run rneovim
    let rneovim_status = Command::new("cargo")
        .args(&["run", "--release", "--", &filename, "--test-keys", keys, "--test-output", &actual_file])
        .status()
        .expect("Failed to run rneovim");
    assert!(rneovim_status.success(), "rneovim failed for test: {}", test_name);

    // 2. Run nvim (headless)
    // We use a temporary binary file to feed keys via -s
    // But we need to make sure nvim doesn't quit too early.
    // A better way for these simple tests is to use -c "normal! ..."
    // For ESC and other special chars, we can use :exe "normal! ..." with \<Esc>
    
    let processed_keys_for_nvim = keys
        .replace("\\x1b", "\\<Esc>")
        .replace("\\x16", "\\<C-v>")
        .replace("\\x04", "\\<C-d>")
        .replace("\\x15", "\\<C-u>");

    let nvim_status = Command::new("nvim")
        .args(&[
            "--headless", "-u", "NONE", "-i", "NONE",
            "-c", &format!("edit {}", filename),
            "-c", &format!("execute \"normal! {}\"", processed_keys_for_nvim),
            "-c", &format!("w! {}", expected_file),
            "-c", "qa!",
        ])
        .status()
        .expect("Failed to run nvim");
    assert!(nvim_status.success(), "nvim failed for test: {}", test_name);

    // 3. Compare results
    let actual = fs::read_to_string(&actual_file).expect("Failed to read actual output");
    let expected = fs::read_to_string(&expected_file).expect("Failed to read expected output");

    if actual != expected {
        println!("FAILURE in test: {}", test_name);
        println!("Initial content: {:?}", initial_content);
        println!("Keys (raw): {:?}", keys);
        println!("Keys (nvim): {:?}", processed_keys_for_nvim);
        println!("--- ACTUAL ---");
        println!("{}", actual);
        println!("--- EXPECTED ---");
        println!("{}", expected);
        
        // Clean up before panicking
        let _ = fs::remove_file(&filename);
        let _ = fs::remove_file(&actual_file);
        let _ = fs::remove_file(&expected_file);
        
        panic!("Results differ between rneovim and nvim");
    }

    // Clean up
    let _ = fs::remove_file(&filename);
    let _ = fs::remove_file(&actual_file);
    let _ = fs::remove_file(&expected_file);
}

#[test]
fn test_dw_simple() {
    assert_nvim_compat("print('hello world')\n", "dw", "dw_simple");
}

#[test]
fn test_dw_multiline() {
    assert_nvim_compat("abc\ndef\n", "dw", "dw_multiline");
}

#[test]
fn test_ciw_undo_x() {
    // ciw, ESC, u, x
    assert_nvim_compat("print('hello world')\n", "ciw\\x1bux", "ciw_undo_x");
}

#[test]
fn test_block_insert() {
    // Ctrl-v (\x16), G, I, #, ESC (\x1b)
    assert_nvim_compat("line1\nline2\nline3\n", "\\x16GI#\\x1b", "block_insert");
}

#[test]
fn test_block_insert_undo() {
    let content = std::fs::read_to_string("sample.txt").expect("sample.txt not found");
    // j, j, Ctrl-v (\x16), j, j, j, j, j, I, !, ESC (\x1b), u
    assert_nvim_compat(&content, "jj\\x16jjjjjI!\\x1bu", "block_insert_undo");
}

#[test]
fn test_cw_special() {
    // cw on a word should not delete the following space
    assert_nvim_compat("import request\n", "cw\\x1b", "cw_special");
}

#[test]
fn test_dw_undo_multiple() {
    // Use sample.txt content
    let content = std::fs::read_to_string("sample.txt").expect("sample.txt not found");
    // dw, u, u, u
    assert_nvim_compat(&content, "dwuuu", "dw_undo_multiple");
}

#[test]
fn test_excessive_undo() {
    let content = std::fs::read_to_string("sample.txt").expect("sample.txt not found");
    // Five undos on a fresh buffer should not change anything
    assert_nvim_compat(&content, "uuuuu", "excessive_undo");
}

#[test]
fn test_search_open_undo() {
    let content = std::fs::read_to_string("sample.txt").expect("sample.txt not found");
    // /print<CR>oprint<ESC>u
    // \x0d is <CR>, \x1b is <Esc>
    assert_nvim_compat(&content, "/print\\x0doprint\\x1bu", "search_open_undo");
}

#[test]
fn test_delete_count() {
    let content = std::fs::read_to_string("sample.txt").expect("sample.txt not found");
    // d5d: delete 5 lines from the start
    assert_nvim_compat(&content, "d5d", "delete_count");
}

#[test]
fn test_insert_undo_simple() {
    assert_nvim_compat("hello\n", "i123\\x1bu", "insert_undo_simple");
}

#[test]
fn test_change_undo_simple() {
    assert_nvim_compat("hello world\n", "cw123\\x1bu", "change_undo_simple");
}

#[test]
fn test_sample_file_edit() {
    let content = "import requests

# Send request to download Ramanujan's formula documentation
url = \"https://en.wikipedia.org/wiki/Summation_of_eulerian_numbers\"
response = requests.get(url)

# Parse HTML content using BeautifulSoup
from bs4 import BeautifulSoup
soup = BeautifulSoup(response.content, 'html.parser')

# Extract relevant formulas and equations from the webpage
formulas = soup.find_all('span', {'class': 'latex'})

for formula in formulas:
    print(formula.text)
";
    // Operations:
    // jjj (line 4) dw (delete 'url') -> NO, line 4 is url = ...
    // jjj: line 1(import), 2(empty), 3(# Send), 4(url)
    // let's do:
    // jjjdw (line 4, delete 'url ')
    // Gdd (delete last line)
    // 5G (line 5) -> No 5G.
    // jjjjj (line 6) dw (delete 'response ')
    assert_nvim_compat(content, "jjjdwjjjjdwGdd", "sample_file_edit");
}
