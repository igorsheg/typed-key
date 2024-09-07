// @ts-check
import antfu from '@antfu/eslint-config'

export default antfu(
  {
    ignores: [
      'crates',
      'Cargo.toml',
      'src/fixture/src/assets',
      // eslint ignore globs here
    ],
  },
  {
    rules: {
      // overrides
    },
  },
)
