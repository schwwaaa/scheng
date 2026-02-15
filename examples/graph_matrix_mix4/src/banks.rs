use std::path::PathBuf;

use scheng_runtime::BankSet;

pub fn print_bank(banks: &BankSet, bank_idx: usize) {
    let bank = &banks.banks[bank_idx];
    eprintln!("[bank] {} ({} scenes)", bank.name, bank.scenes.len());
    for (i, s) in bank.scenes.iter().enumerate() {
        eprintln!("  {}: {}  -> {}", i + 1, s.name, s.preset.name());
    }
}

pub fn parse_args_banks_path() -> Option<PathBuf> {
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--banks" {
            if let Some(p) = it.next() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}
