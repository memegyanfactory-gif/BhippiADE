# Security policy

## Supported versions

Bhippi is currently pre-release software. Security fixes are applied to the latest `main` branch.

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability or exposed credential.

Use GitHub's private vulnerability reporting flow from this repository's **Security** tab. Include:

- The affected component and revision.
- Reproduction steps or a minimal proof of concept.
- The expected and observed impact.
- Any suggested mitigation.

Avoid including real API keys, private user data, or unrelated system information. Replace secrets with clearly marked test values.

## Security boundaries

Bhippi treats model output, fetched content, imported assets, gameplay scripts, and provider processes as untrusted inputs. Security-sensitive behavior is enforced in Rust through typed commands, explicit capabilities, bounded execution, path confinement, secret scrubbing, transactional writes, and blocking release gates.
