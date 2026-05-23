# Contributing

Thanks for your interest in contributing. This project is MIT-licensed; by
contributing you agree your work is provided under that same license.

## Developer Certificate of Origin (DCO)

All commits must be signed off. The sign-off is a one-line trailer at the
end of the commit message certifying that you wrote the patch (or have the
right to submit it) under the project's open source license. The full text
is at <https://developercertificate.org>.

To sign off, add `-s` to your commit:

```
git commit -s -m "your message"
```

This appends a `Signed-off-by: Your Name <your@email>` line. Use your real
name; pseudonymous sign-offs are not accepted.

## Workflow

1. Fork the repo and create a topic branch.
2. Make your change. Keep PRs focused — one concern per PR.
3. Run `cargo check` and `cargo build --target wasm32-unknown-unknown`
   before opening the PR.
4. Open a PR against `main`. Describe what you changed and why.

## Scope

See `CLAUDE.md` and `README.md` for architecture notes and conventions
(integer cents, no `unsafe`, no heavy WASM deps, soft errors as HTTP 200,
etc.). Changes that violate those conventions will be asked to revise.
