mod git_command;
mod models;

use anyhow::{Error, Result};
use clap::Parser;
use git_command::{GitCommand, GitCommandTrait};
use models::Regexes;
use semver::{BuildMetadata, Prerelease, Version};
use serde_json::{json, to_string_pretty, Value};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Useful for monorepos with mutiple versionable applications. Tags and release branches will have to be prefixed with an application name. E.g. tag: `app-1.0.0`, branch: `release/app-1.0.0`.
    #[arg(short, long)]
    app_name: Option<String>,

    /// Build number to be included in the SemVer build metadata. Often used when using a build system. When not provided, the git commit count for the branch is used.
    #[arg(short, long)]
    build_nubmer: Option<u32>,

    /// Skip fetching (impoves performance for local runs, but may result in outdated version information)
    #[arg(short, long, action)]
    skip_fetch: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let git_command = GitCommand {};
    let version_output = get_version_output(&args, &git_command)?;
    println!("{}", to_string_pretty(&version_output)?);
    Ok(())
}

fn get_version_output(args: &Args, git_command: &impl GitCommandTrait) -> Result<Value, Error> {
    let regexes = Regexes::new(&args.app_name)?;
    if !args.skip_fetch {
        git_command.run(vec!["fetch", "--tags"])?;
    }
    let git_branch = git_command.run(vec!["branch", "--show-current"])?;
    let git_rev = git_command.run(vec!["rev-parse", "--short", "HEAD"])?;
    let rev_count = git_command.run(vec!["rev-list", "--count", "HEAD"])?;
    let semver = get_version(git_command, &regexes, &git_branch, args)?;
    let new_semver = update_version(
        &git_branch,
        &regexes,
        &git_rev,
        get_count(args, &rev_count)?,
        &semver,
    )?;
    let version_output = json!({
        "git_branch": git_branch,
        "git_rev": git_rev,
        "rev_count": rev_count,
        "app_version": new_semver.to_string(),
        "container_tag": new_semver.to_string().replace('+', ".")
    });
    Ok(version_output)
}

fn update_version(
    git_branch: &str,
    regexes: &Regexes,
    git_rev: &String,
    counter: u32,
    semver: &Version,
) -> Result<Version> {
    let mut new_semver = semver.clone();
    if regexes.main_branches.is_match(git_branch) {
        new_semver.build = BuildMetadata::new(git_rev)?;
    } else if regexes.rc_branches.is_match(git_branch) {
        new_semver.pre = Prerelease::new(&format!("rc.{}", counter)).unwrap();
        new_semver.build = BuildMetadata::new(git_rev)?;
    } else if regexes.develop_branches.is_match(git_branch) {
        new_semver.patch += 1;
        new_semver.pre = Prerelease::new(&format!("beta.{}", counter)).unwrap();
        new_semver.build = BuildMetadata::new(git_rev)?;
    } else {
        new_semver.patch += 1;
        new_semver.pre = Prerelease::new(&format!("alpha.{}", counter)).unwrap();
        let escaped_branch = regexes.escape_branch.replace_all(git_branch, "-");
        if escaped_branch.len() > 50 {
            escaped_branch.to_string().truncate(50);
        }
        new_semver.build = BuildMetadata::new(&format!("{}.{}", escaped_branch, &git_rev))?;
    };
    Ok(new_semver)
}

fn get_count(args: &Args, rev_count: &str) -> Result<u32, Error> {
    let counter = if args.build_nubmer.is_some() {
        args.build_nubmer.unwrap()
    } else {
        rev_count.parse::<u32>()?
    };
    Ok(counter)
}

fn get_version(
    git_command: &impl GitCommandTrait,
    regexes: &Regexes,
    git_branch: &str,
    args: &Args,
) -> Result<Version> {
    let tag: String;
    let version: String;
    let semver: Version;
    // For release branches, get the version from the branch name
    if regexes.rc_branches.is_match(git_branch) {
        let caps = regexes
            .rc_branches
            .captures(git_branch)
            .ok_or(Error::msg("Invalid branch name format"))?;
        version = caps.name("version").unwrap().as_str().to_string();
        semver = Version::parse(&version)?;
    } else {
        // For all other branches, get the version from the latest tag
        let get_tags_result = if args.app_name.is_none() {
            git_command.run(vec!["describe", "--abbrev=0", "--tags"])
        } else {
            git_command.run(vec![
                "describe",
                "--abbrev=0",
                "--match",
                format!("{}-*", args.app_name.as_ref().unwrap()).as_str(),
                "--tags",
            ])
        };

        // Fall back to 0.0.0 if no tags are found
        match get_tags_result {
            Ok(t) => tag = t,
            Err(_) => {
                tag = if args.app_name.is_none() {
                    "0.0.0".to_string()
                } else {
                    format!("{}-0.0.0", args.app_name.as_ref().unwrap())
                }
            }
        }

        // For the main branch, a tag must exist on the current commit
        if regexes.main_branches.is_match(git_branch) {
            let extact_tag =
                git_command.run(vec!["describe", "--abbrev=0", "--exact-match", "--tags"])?;
            if extact_tag != tag {
                return Err(Error::msg(
                    "Cannot version a production release from a commit without a tag",
                ));
            }
        }

        // Extract the semver version from the tag
        let caps = regexes
            .tag
            .captures(&tag)
            .ok_or(Error::msg("No tag found"))?;
        version = caps.name("version").unwrap().as_str().to_string();
        semver = Version::parse(&version).map_err(|err| {
            Error::msg(format!(
                "Tag '{}' cannot be parsed to SemVer Version.\nDo you have app names in your tags? Provide the '--app-name' option.\nError: '{}'",
                tag, err
            ))
        })?;
    }
    Ok(semver)
}

#[cfg(test)]
mod tests {
    use crate::git_command::MockGitCommandTrait;

    use super::*;

    #[test]
    fn test_get_version_main_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "main";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: Some(app_name.unwrap().to_owned()),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse(&version.clone().unwrap()).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_main_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "main";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_main_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "main";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse(&version.clone().unwrap()).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_main_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "main";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_develop_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "develop";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse(&version.clone().unwrap()).unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("beta.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_develop_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "develop";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("beta.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();

        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_develop_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "develop";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse(&version.clone().unwrap()).unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("beta.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_develop_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "develop";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("beta.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();

        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_release_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "release/myapp-1.1.0";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.1.0").unwrap();
        expected_version.pre = Prerelease::new(&format!("rc.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_release_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "release/myapp-1.1.0";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.1.0").unwrap();
        expected_version.pre = Prerelease::new(&format!("rc.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_release_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "release/1.1.0";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.1.0").unwrap();
        expected_version.pre = Prerelease::new(&format!("rc.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_release_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "release/1.1.0";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.1.0").unwrap();
        expected_version.pre = Prerelease::new(&format!("rc.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&rev).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_feature_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "feature/feat-1";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("alpha.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&format!("feature-feat-1.{}", rev)).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_feature_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name = Some("myapp");
        let branch = "feature/feat-1";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: Some(String::from("myapp")),
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("alpha.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&format!("feature-feat-1.{}", rev)).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_feature_branch_with_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "feature/feat-1";
        let rev = "1234567";
        let count = "1";
        let version = Some("1.0.0");

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("1.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("alpha.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&format!("feature-feat-1.{}", rev)).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    #[test]
    fn test_get_version_no_app_name_feature_branch_without_tag() {
        let mut git_command = MockGitCommandTrait::new();
        let app_name: Option<&str> = None;
        let branch = "feature/feat-1";
        let rev = "1234567";
        let count = "1";
        let version = None;

        let args = Args {
            app_name: None,
            build_nubmer: None,
            skip_fetch: false,
        };

        mock_git(&mut git_command, app_name, branch, rev, count, version);

        let result = get_version_output(&args, &git_command);

        assert!(result.is_ok());

        let output = result.unwrap();

        let mut expected_version = Version::parse("0.0.0").unwrap();
        expected_version.patch += 1;
        expected_version.pre = Prerelease::new(&format!("alpha.{}", count)).unwrap();
        expected_version.build = BuildMetadata::new(&format!("feature-feat-1.{}", rev)).unwrap();
        assert_expected_version(branch, rev, count, expected_version, output);
    }

    fn mock_git<'a>(
        git_command: &mut MockGitCommandTrait,
        app_name: Option<&'a str>,
        branch: &'a str,
        rev: &'a str,
        count: &'a str,
        version: Option<&'a str>,
    ) where
        'a: 'static,
    {
        git_command
            .expect_run()
            .withf(|args| args[0] == "fetch" && args[1] == "--tags")
            .returning(|_| Ok(String::from("")));

        git_command
            .expect_run()
            .withf(|args| args[0] == "branch" && args[1] == "--show-current")
            .returning(|_| Ok(branch.to_string()));

        git_command
            .expect_run()
            .withf(|args| args[0] == "rev-parse" && args[1] == "--short" && args[2] == "HEAD")
            .returning(|_| Ok(rev.to_string()));

        git_command
            .expect_run()
            .withf(|args| args[0] == "rev-list" && args[1] == "--count" && args[2] == "HEAD")
            .returning(move |_| Ok(count.to_string()));

        let exact_version: &str;
        if version.is_some() {
            if app_name.is_none() {
                git_command
                    .expect_run()
                    .withf(|args| {
                        args[0] == "describe" && args[1] == "--abbrev=0" && args[2] == "--tags"
                    })
                    .returning(move |_| Ok(version.unwrap().to_string()));
            } else {
                git_command
                    .expect_run()
                    .withf(move |args| {
                        args[0] == "describe"
                            && args[1] == "--abbrev=0"
                            && args[2] == "--match"
                            && args[3] == format!("{}-*", app_name.unwrap())
                            && args[4] == "--tags"
                    })
                    .returning(move |_| Ok(format!("{}-{}", app_name.unwrap(), version.unwrap())));
            }
            exact_version = version.unwrap();
        } else {
            if app_name.is_none() {
                git_command
                    .expect_run()
                    .withf(|args| {
                        args[0] == "describe" && args[1] == "--abbrev=0" && args[2] == "--tags"
                    })
                    .returning(|_| Err(Error::msg("No tag found")));
            } else {
                git_command
                    .expect_run()
                    .withf(move |args| {
                        args[0] == "describe"
                            && args[1] == "--abbrev=0"
                            && args[2] == "--match"
                            && args[3] == format!("{}-*", app_name.unwrap())
                            && args[4] == "--tags"
                    })
                    .returning(|_| Err(Error::msg("No tag found")));
            }
            exact_version = "0.0.0";
        }
        if app_name.is_none() {
            git_command
                .expect_run()
                .withf(|args| {
                    args[0] == "describe"
                        && args[1] == "--abbrev=0"
                        && args[2] == "--exact-match"
                        && args[3] == "--tags"
                })
                .returning(move |_| Ok(exact_version.to_string()));
        } else {
            git_command
                .expect_run()
                .withf(move |args| {
                    args[0] == "describe"
                        && args[1] == "--abbrev=0"
                        && args[2] == "--exact-match"
                        && args[3] == "--tags"
                })
                .returning(move |_| Ok(format!("{}-{}", app_name.unwrap(), exact_version)));
        }
    }

    fn assert_expected_version(
        branch: &str,
        rev: &str,
        count: &str,
        expected_version: Version,
        output: Value,
    ) {
        let expected_output = json!(
            {
                "git_branch": branch,
                "git_rev": rev,
                "rev_count": count,
                "app_version":  format!("{}", expected_version),
                "container_tag": format!("{}", expected_version).replace('+', ".")
            }
        );
        assert_eq!(output, expected_output);
    }
}
