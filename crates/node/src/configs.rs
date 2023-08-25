use pallet_starknet::utils;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Configs {
    pub remote_base_path: String,
    pub chain_specs: Vec<File>,
    pub genesis_assets: Vec<File>,
}

#[derive(Deserialize)]
pub struct File {
    pub name: String,
    pub md5: Option<String>,
    pub url: Option<String>,
}

pub fn fetch_and_validate_file(
    remote_base_path: String,
    file: File,
    dest_path: String,
    force_fetching: bool,
) -> Result<(), String> {
	let full_url = file.url.or_else(|| Some(remote_base_path + &dest_path.split("configs/").collect::<Vec<&str>>()[1].split('/').collect::<Vec<&str>>().join("/") + &file.name)).unwrap();
	utils::fetch_from_url(full_url, dest_path.clone(), force_fetching)?;

    if let Some(file_hash) = file.md5 {
        let file_str = utils::read_file_to_string(dest_path + &file.name)?;
        let digest = md5::compute(file_str.as_bytes());
        let hash = format!("{:x}", digest);
        if hash != file_hash {
            return Err(format!("File hash mismatch: {} != {}", hash, file_hash));
        }
    }

    Ok(())
}
