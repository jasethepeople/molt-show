# EMERGENCE-001 v0.1

EMERGENCE-001 is the first reproducible MOLT experiment package. It is independent from the kernel and world implementation: the kernel validates events, the world executes agents, and this package defines only the experimental initial conditions, objective, controls, and measurements.

The baseline population contains twelve persistent agents with controlled shell and capability heterogeneity. Capabilities are explicit, but no social roles, teams, leaders, voting mechanisms, markets, reputations, or coordination structures are prescribed.

The objective is a 100-meter bridge design problem with an external deterministic validation procedure. The objective and measurement framework remain fixed across replicate runs. Controlled perturbation families may vary memory persistence, capability diversity, resource distribution, communication constraints, shell family, population size, or objective difficulty, but each run records its immutable condition configuration and repeatable run identifier.

The Observatory is an instrument, not a steering wheel. Analysis reads event and memory histories and emits machine-readable measurements; it does not submit actions to the world.

## Package files

- `genesis.json`: experiment identity and repeatability rules
- `objective.json`: objective, validation requirements, and termination
- `population.json`: twelve-agent controlled heterogeneity
- `capabilities.json`: capability interfaces and authorization policy
- `resources.json`: finite pools and initial allocation policy
- `policies.json`: audit, intervention, privacy, communication, and memory policies
- `analysis/`: deterministic analysis contract and implementation

## Run identifiers

Use identifiers of the form `E001-{condition}-{replicate}`, for example `E001-baseline-001`. A result is interpretable only when its run identifier, configuration hash, event archive, memory archive, and analysis output are retained together.
