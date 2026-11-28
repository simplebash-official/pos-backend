# Running the checks on Windows (WSL2)

`make check` needs `make`, `bash` and Docker, so on Windows run it inside WSL2,
not in PowerShell or Command Prompt. The [deploy checklist](deploy-checklist.md)
then applies unchanged.

These steps have not been tried on a Windows machine yet.

## One-time setup

### 1. Install WSL2

In PowerShell **as Administrator**:

```powershell
wsl --install -d Ubuntu
```

Restart if asked, open **Ubuntu** from the Start menu and create your Linux user.
Then check in PowerShell that the `VERSION` column shows `2`:

```powershell
wsl -l -v
```

If it shows `1`, run `wsl --set-version Ubuntu 2`.

### 2. Connect Docker Desktop

Install Docker Desktop on Windows, then in its settings:

- **General:** turn on **Use the WSL 2 based engine**.
- **Resources > WSL integration:** turn on **Ubuntu**.

Do not install a second Docker inside Ubuntu. In the Ubuntu terminal, this must
work:

```bash
docker info
```

**Every command from here on runs in the Ubuntu terminal.**

### 3. Install the tools

```bash
sudo apt update
```

```bash
sudo apt install -y build-essential pkg-config git curl ca-certificates gh
```

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
source "$HOME/.cargo/env"
```

```bash
rustup target add wasm32-unknown-unknown
```

Rust installed on Windows is not visible inside WSL, so this installs it again.

### 4. Sign in to GitHub

```bash
gh auth login
```

Also set `git config --global user.name` and `user.email`.

### 5. Clone the repos

Clone into your Linux home folder, **not** under `/mnt/c`, where builds are much
slower. `document-server` must sit beside the backend.

```bash
mkdir -p ~/simplebash/pos && cd ~/simplebash/pos
```

```bash
git clone https://github.com/simplebash-official/pos-backend.git backend
```

```bash
git clone https://github.com/simplebash-official/document-server.git
```

No `.env` file is needed for the checks.

## Every time

1. Start Docker Desktop on Windows.
2. Open the Ubuntu terminal:

   ```bash
   cd ~/simplebash/pos/backend
   ```

3. Follow the [deploy checklist](deploy-checklist.md), including `make check`.

Edit, commit and push from this clone. A separate clone on the Windows drive may
hold different code from what you tested. To edit it in VS Code, install the
**WSL** extension and run `code .` in this folder.

The first run downloads the MongoDB image and compiles everything, so it takes
much longer than later runs.

## Problems

| What you see | What to do |
|---|---|
| `Docker is not running` | Start Docker Desktop and check the WSL integration setting |
| `docker: command not found` | Turn on the WSL integration for Ubuntu, then reopen the terminal |
| `cargo: command not found` | Run `source "$HOME/.cargo/env"` or reopen the terminal |
| Container name already taken | See the [deploy checklist](deploy-checklist.md) |
| `$'\r': command not found` | The clone has Windows line endings; clone again inside WSL |
