# Rustclaw

Rustclaw is a terminal AI agent focused on local workflows, project context, and developer-friendly tooling.

## Installation

### Linux

1. Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Restart your shell or load Cargo manually:

```bash
source "$HOME/.cargo/env"
```

3. Clone the repository:

```bash
git clone https://github.com/lautiromero/rustclaw
cd rustclaw
```

4. Optional: install the recommended terminal editor, Micro:

```bash
curl https://getmic.ro | bash
```

Rustclaw's visual mode can also use any editor configured in your `EDITOR` environment variable.

5. Run the installer:

```bash
./install.sh
```

If the script is not executable, run:

```bash
chmod +x install.sh
./install.sh
```

6. Configure an OpenAI-compatible API endpoint in `~/.rustclaw/config.toml`:

```toml
[llm]
openai_base_url = "http://localhost:8080/v1" # Ex: https://integrate.api.nvidia.com/v1
model = "gpt-5.5"
```

7. Configure your API key in `~/.rustclaw/.env` if needed:

```env
OPENAI_API_KEY=your_api_key_here
```
