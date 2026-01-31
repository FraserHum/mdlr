# Auto-Improve

Run `mdlr prompt` to get instructions for improving the codebase, then follow those instructions.

## Steps

1. Run `mdlr prompt` and read the output carefully
2. Follow the instructions provided in the prompt output, creating a plan and considering alternatives.
3. Follow the plan to make the suggested improvements to the codebase
4. Ensure all existing tests continue to pass by running `cargo test`
5. Update or add tests as needed to cover your changes
6. If you add a new metric, CLI command, or language support, update the relevant documentation as specified in CLAUDE.md
