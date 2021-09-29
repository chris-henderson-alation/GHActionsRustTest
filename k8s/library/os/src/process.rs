use result::Result;
use std::process::Stdio;
use std::string::FromUtf8Error;
use tokio::process::Command;

use error::*;
use tokio::io::AsyncWriteExt;

/// cmd runs any arbitrary system command asynchronously and returns the resulting stdout.
/// The returned stdout is guaranteed to not have any trailing newlines or spaces.
///
/// Stderr will be included in any returned error should a stderr be available.
///
/// ```ignore
/// cmd!("ls").await.unwrap();
/// cmd!("ls", "-al").await.unwarp();
/// let temp = cmd!("mktemp").await.unwrap();
/// let contents = cmd!("cat", &temp).await.unwrap();
/// ```
#[macro_export]
macro_rules! cmd {
    (stdin=$stdin:expr, $command:expr) => {
        {
            let cmd = tokio::process::Command::new($command);
            let debug_string: String = format!("{}", $command);
            os::process::exec(Some($stdin), cmd, debug_string)
        }
    };
    (stdin=$stdin:expr, $command:expr $(,$args:expr)*) => {
        {
            let mut cmd = tokio::process::Command::new($command);
            $(cmd.arg($args);)*
            let mut debug_string: Vec<String> = vec![format!("{}", $command)];
            $(
                debug_string.push(format!("{}", $args));
            )*
            let debug_string: String = debug_string.join(" ");
            os::process::exec(Some($stdin), cmd, debug_string)
        }
    };
    ($command:expr) => {
        {
            let cmd = tokio::process::Command::new($command);
            let debug_string: String = format!("{}", $command);
            os::process::exec(None::<&[u8]>, cmd, debug_string)
        }
    };
    ($command:expr $(,$args:expr)*) => {
        {
            let mut cmd = tokio::process::Command::new($command);
            $(cmd.arg($args);)*
            let mut debug_string: Vec<String> = vec![format!("{}", $command)];
            $(
                debug_string.push(format!("{}", $args));
            )*
            let debug_string: String = debug_string.join(" ");
            os::process::exec(None::<&[u8]>, cmd, debug_string)
        }
    }
}

pub async fn exec<S: AsRef<[u8]>>(
    stdin: Option<S>,
    mut cmd: Command,
    debug_string: String,
) -> Result<String> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(if let Some(_) = stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().map_err(|err| FailedToSpawn {
        command: debug_string.clone(),
        source: err,
    })?;
    if let Some(stdin) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_ref())
            .await
            .unwrap();
    };
    let output = child.wait_with_output().await.map_err(|err| FailedToRun {
        command: debug_string.clone(),
        source: err,
    })?;
    if !output.status.success() {
        let stderr_result = String::from_utf8(output.stderr.clone());
        let stderr = stderr_result.map_err(|err| InvalidUTF8Stderr {
            command: debug_string.clone(),
            output: format!("{}", String::from_utf8_lossy(&output.stderr)),
            source: err,
        })?;
        return Err(CommandFailed {
            command: debug_string.clone(),
            stderr,
        }
        .into());
    }
    let stdout_result = String::from_utf8(output.stdout.clone());
    let stdout = stdout_result.map_err(|err| InvalidUTF8 {
        command: debug_string.clone(),
        output: format!("{}", String::from_utf8_lossy(&output.stdout)),
        source: err,
    })?;
    Ok(stdout.trim_end().to_string())
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error(
r#"Failed to spawn the "{command}" command. Perhaps the ACM is corrupted? Perhaps try destroying its pod?"#
)]
#[code(Status::InternalServerError)]
struct FailedToSpawn {
    command: String,
    #[source]
    source: std::io::Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error(r#"Stdout for "{command}" was not valid UTF-8, got the following (lossy) {output}"#)]
#[code(Status::InternalServerError)]
struct InvalidUTF8 {
    command: String,
    // This is going to be a "lossy" string so it
    // might have some strange looking runes in it.
    output: String,
    #[source]
    source: FromUtf8Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error(r#"Stderr for "{command}" was not valid UTF-8, got the following (lossy) {output}"#)]
#[code(Status::InternalServerError)]
struct InvalidUTF8Stderr {
    command: String,
    // This is going to be a "lossy" string so it
    // might have some strange looking runes in it.
    output: String,
    #[source]
    source: FromUtf8Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error(r#"Failed to execute the "{command}" command. Perhaps the ACM is corrupted? Perhaps try destroying its pod?"#)]
#[code(Status::InternalServerError)]
struct FailedToRun {
    command: String,
    #[source]
    source: std::io::Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error(r#"Failed to execute "{command}". Stderr was {stderr}"#)]
#[code(Status::InternalServerError)]
struct CommandFailed {
    command: String,
    stderr: String,
}

#[cfg(test)]
mod tests {

    use crate as os;

    #[tokio::test]
    async fn ls() {
        println!("{}", cmd!("ls").await.unwrap());
    }

    #[tokio::test]
    async fn ls_al() {
        println!("{}", cmd!("ls", "-a", "-l").await.unwrap());
    }

    #[tokio::test]
    async fn no_newline() {
        let ls = cmd!("ls", "/").await.unwrap();
        assert!(!ls.is_empty());
        assert!(!ls.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_stdin() {
        assert_eq!("hello!", cmd!(stdin = "hello!", "cat").await.unwrap());
    }
}
