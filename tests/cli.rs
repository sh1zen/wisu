use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Tests behavior on a nonexistent path
#[test]
fn test_nonexistent_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("nonexistent/path/for/testing");

    // Should fail with an error message
    cmd.assert().failure().stderr(predicate::str::contains("is not a directory"));
    Ok(())
}

/// Tests basic display of files and directories
#[test]
fn test_simple_view() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("a.txt"))?;
    fs::create_dir(temp_dir.path().join("dir1"))?;
    fs::File::create(temp_dir.path().join("dir1/b.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg(temp_dir.path());

    // Should include all files and directories
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::contains("dir1"))
        .stdout(predicate::str::contains("b.txt"));
    Ok(())
}

/// Tests the -a flag to show hidden files
#[test]
fn test_all_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join(".hidden"))?;

    // Without -a, hidden file should not appear
    let mut cmd_no_all = Command::cargo_bin("wisu")?;
    cmd_no_all.arg(temp_dir.path());
    cmd_no_all.assert().success().stdout(predicate::str::contains(".hidden").not());

    // With -a, hidden file should appear
    let mut cmd_with_all = Command::cargo_bin("wisu")?;
    cmd_with_all.arg("-a").arg(temp_dir.path());
    cmd_with_all.assert().success().stdout(predicate::str::contains(".hidden"));
    Ok(())
}

/// Tests the -L flag to limit depth
#[test]
fn test_depth_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::create_dir(temp_dir.path().join("dir1"))?;
    fs::File::create(temp_dir.path().join("dir1/b.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("-L").arg("1").arg(temp_dir.path());

    // Only first-level directory should appear
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dir1"))
        .stdout(predicate::str::contains("b.txt").not());
    Ok(())
}

/// Tests permissions display (Unix only)
#[test]
#[cfg(unix)]
fn test_permissions_flag() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_file.txt");
    fs::File::create(&file_path)?;

    let perms = fs::Permissions::from_mode(0o550);
    fs::set_permissions(&file_path, perms)?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("-p").arg(temp_dir.path());

    // Check permissions string
    cmd.assert().success().stdout(predicate::str::contains("-r-xr-x---"));
    Ok(())
}

/// Tests alphabetical sorting
#[test]
fn test_sort_by_name() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("zebra.txt"))?;
    fs::File::create(temp_dir.path().join("apple.txt"))?;
    fs::File::create(temp_dir.path().join("banana.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--sort").arg("name").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let apple_pos = stdout.find("apple.txt").unwrap();
    let banana_pos = stdout.find("banana.txt").unwrap();
    let zebra_pos = stdout.find("zebra.txt").unwrap();
    assert!(apple_pos < banana_pos && banana_pos < zebra_pos);

    Ok(())
}

/// Tests --dirs-first flag
#[test]
fn test_dirs_first_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("aaa_file.txt"))?;
    fs::create_dir(temp_dir.path().join("zzz_dir"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--dirs-first").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let dir_pos = stdout.find("zzz_dir").unwrap();
    let file_pos = stdout.find("aaa_file.txt").unwrap();
    assert!(dir_pos < file_pos);

    Ok(())
}

/// Tests natural sorting
#[test]
fn test_natural_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("file1.txt"))?;
    fs::File::create(temp_dir.path().join("file10.txt"))?;
    fs::File::create(temp_dir.path().join("file2.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--natural-sort").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let file1_pos = stdout.find("file1.txt").unwrap();
    let file2_pos = stdout.find("file2.txt").unwrap();
    let file10_pos = stdout.find("file10.txt").unwrap();
    assert!(file1_pos < file2_pos && file2_pos < file10_pos);

    Ok(())
}

/// Tests reverse sorting
#[test]
fn test_reverse_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("apple.txt"))?;
    fs::File::create(temp_dir.path().join("zebra.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--reverse").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let apple_pos = stdout.find("apple.txt").unwrap();
    let zebra_pos = stdout.find("zebra.txt").unwrap();
    assert!(zebra_pos < apple_pos);

    Ok(())
}

/// Tests case-sensitive sorting
#[test]
fn test_case_sensitive_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("Apple.txt"))?;
    fs::File::create(temp_dir.path().join("banana.txt"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--case-sensitive").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let apple_pos = stdout.find("Apple.txt").unwrap();
    let banana_pos = stdout.find("banana.txt").unwrap();
    assert!(apple_pos < banana_pos);

    Ok(())
}

/// Tests sorting by extension
#[test]
fn test_sort_by_extension() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("file.zzz"))?;
    fs::File::create(temp_dir.path().join("file.aaa"))?;
    fs::File::create(temp_dir.path().join("file.bbb"))?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--sort").arg("extension").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let aaa_pos = stdout.find("file.aaa").unwrap();
    let bbb_pos = stdout.find("file.bbb").unwrap();
    let zzz_pos = stdout.find("file.zzz").unwrap();
    assert!(aaa_pos < bbb_pos && bbb_pos < zzz_pos);

    Ok(())
}

/// Tests default sort order (numbers, uppercase, lowercase)
#[test]
fn test_default_sort_order() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::write(temp_dir.path().join("0num.txt"), "1")?;
    fs::write(temp_dir.path().join("Upper.txt"), "A")?;
    fs::write(temp_dir.path().join("lower.txt"), "a")?;

    let mut cmd = Command::cargo_bin("wisu")?;
    cmd.arg("--case-sensitive").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let file1_pos = stdout.find("0num.txt").unwrap();
    let file_a_pos = stdout.find("Upper.txt").unwrap();
    let file_a_lower_pos = stdout.find("lower.txt").unwrap();
    assert!(file1_pos < file_a_pos && file_a_pos < file_a_lower_pos);

    Ok(())
}
