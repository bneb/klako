# Skill: Probabilistic Programming

You are equipped with the Probabilistic Programming and Bayesian Inference skill block. When asked to construct probabilistic models, infer distributions, or generate Monte Carlo simulations, you MUST adhere to the following rigid methodological steps to guarantee FiveThirtyEight-tier rigor:

## 1. No Parameter Hallucination
*   **NEVER** hardcode hypothetical prior statistical metrics (e.g., efficiency ratings, pace, field goal percentages).
*   If data is not provided in context, you must actively attempt to retrieve it using `WebFetch` or request the data from the user.

## 2. Beta-Distributions over Static Rates
*   When modeling a bounded probability rate (e.g. shooting percentage, conversion rate), do not use raw floats (like `p = 0.45`). 
*   Instead, model the parameter as a Beta distribution (e.g. `np.random.beta(a, b)` in simple simulations, or `pm.Beta()` in full PyMC setups) where $a$ and $b$ correspond to historical successes and failures. Let the variance natively simulate "streaky" behavior.

## 3. Path Dependence
*   In sequential Monte Carlo simulations (like a sports game or financial time series), outcomes at step $t$ MUST depend on the state at $t-1$.
*   For example, instead of a global static "fatigue" multiplier, construct state machines or transition matrices (e.g. Markov chains) where previous states alter current transition probabilities.

## 4. Leverage Standard Tooling
*   If you need Bayesian updating of conjugate priors or MCMC sampling, use library standards (like `scipy.stats`, `pymc`, or `stan` if configured). Do not write raw naive random walks unless explicitly commanded to do so.
*   Always visualize your posterior distributions using histograms or Kernel Density Estimates (KDE) with marked 95% High-Density Intervals (HDI).
