## What and why

## Spec
- [ ] This changes no behaviour (refactor, docs, CI), **or**
- [ ] The spec is updated first: `spec/*.md` and `spec/REQUIREMENTS.md` (rule ID: …)

## Checklist
- [ ] `./helper.sh test` passes
- [ ] `./helper.sh lint` passes
- [ ] Golden renderings in `tests/golden/` regenerated and reviewed, if the UI changed
- [ ] `CHANGELOG.md` has a line under *Unreleased*, if users would notice
- [ ] Tried on real hardware, or the change is covered by the simulator (say which)
