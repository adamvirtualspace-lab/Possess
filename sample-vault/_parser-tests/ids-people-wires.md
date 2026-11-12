# Test: IDs, people and wires

> Expected, 4 tasks:
> 1. "Model the boat": ID `boat`, people `lia` and `teo`, no wires.
> 2. "Texture the boat": ID `tex`, person `lia`. Needs `boat` in this file and `r1` in `rig/hero`. The two needs lines add up. It's blocked because `r1` isn't done.
> 3. "Send to client": no ID and no people. The email address is not a mention.
> 4. "Ping ... when textures land": ID `ping`, person `bram` (a mention in the middle of the text still counts), sync from `tex`.

- [x] Model the boat @lia @teo ^boat
- [-] Texture the boat @lia ^tex
  - needs:: [[#^boat]]
  - needs:: [[rig/hero#^r1]]
- [ ] Send to client, cc bob@example.com
- [ ] Ping @bram when textures land ^ping
  - sync:: [[#^tex]]
