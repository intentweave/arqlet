use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("packages/contracts/src/generated"));
    arqlet_contracts::export_to(&dir)?;
    println!("wrote contracts to {}", dir.display());
    Ok(())
}
