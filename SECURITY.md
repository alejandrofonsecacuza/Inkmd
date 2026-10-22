# Security policy

inkmd renders untrusted documents, so it must never let a document
manipulate the terminal or the system. Examples of security issues:

- Content from a Markdown file that reaches the terminal as a control or
  escape sequence (cursor movement, OSC title/clipboard/hyperlink sequences,
  DCS, and so on).
- Execution of anything found in a document, or any network access.
- Reading files outside what the document references as images, or
  mishandling special files.
- Crashes, hangs or unbounded memory use caused by crafted input.

## Reporting a vulnerability

Please **do not open a public issue**. Instead, use GitHub's private
vulnerability reporting: *Security* tab → *Report a vulnerability*. Include a
minimal document that reproduces the problem, the command line, and your
terminal and version.

You should receive a reply within a few days. Once the issue is fixed, it will
be credited in the release notes unless you prefer otherwise.

## Supported versions

Security fixes are made for the latest release only.
