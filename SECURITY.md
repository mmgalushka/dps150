# Security policy

dps150 runs on your computer and talks to a USB device. It opens no network ports and sends nothing over the network.

The part that matters most is **output safety**: a bug that could turn the output on, raise it, or leave it on when the rules in [spec/SAFETY.md](spec/SAFETY.md) say it must go off can damage equipment. Please treat those as security problems.

## Reporting

Use GitHub's private reporting: **Security → Report a vulnerability** on <https://github.com/mmgalushka/dps150/security/advisories/new>. Please do not open a public issue for it.

Include the version (`dps150 --version`), your OS, and the steps. If it reproduces on the simulator, say so.

This is a one-person hobby project, so a reply may take a few days. Only the latest release is supported.
