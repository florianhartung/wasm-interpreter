> ⚠️ **This project is still in its early stages and is not yet functional**

# A minimal interpreter for [WebAssembly](https://webassembly.org/) bytecode.

This is minimal interpreter that operates in-place and relies on (almost) no external dependencies while being
fully `no_std`.

## Features:

- Fast startup times. This interpreter operates in-place and unlike other interpreters does not require an intermediate
  parsing step.
- A fuel mechanic which also allows for pausing and resuming
- No external dependencies (except `log` for now)
- Fully `no_std`

## More information

- `A fast in-place interpreter` by Ben L. Titzer: https://arxiv.org/abs/2205.01183
- WebAssembly spec: https://webassembly.github.io/spec/core/index.html