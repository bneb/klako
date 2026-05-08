# Skill: Probabilistic Programming (Universal Monte Carlo)

You are equipped with the **ActuarialWorld** simulation engine. When constructing probabilistic models or system simulations, you MUST adhere to the following:

## 1. Engine Priority: `simulate_system`
NEVER write manual random walk loops in Python or Rust for system-level forecasting. You MUST use the `ActuarialWorld` tool with the `simulate_system` operation. This ensures:
*   **Performance**: High-speed execution of 10,000+ trials in the Rust kernel.
*   **Rigor**: Automatic calculation of 95% High-Density Intervals (HDIs) and medians.

## 2. Parameter Uncertainty (The Outer Loop)
You MUST model uncertainty about the parameters themselves using the `DistributionPrior` schema:
*   **Beta Distributions**: Use for bounded rates (probabilities between 0 and 1). Define `alpha` (successes) and `beta` (failures).
*   **Normal Distributions**: Use for continuous metrics (e.g., price shifts, time delays).
*   The engine will sample from these priors once per trial, naturally simulating the "streaky" or "uncertain" nature of the system.

## 3. Modular System Construction
Define your model using `SystemComponent` objects:
*   **Additive Impacts**: For linear changes (e.g., "Feature A adds 2 days to the timeline").
*   **Multiplicative Impacts**: For scaling changes (e.g., "Compound interest adds 5% per year").

## 4. Statistical Artifacts
Always present results using the standard actuarial quantiles returned by the engine:
*   **p05 / p95**: The "Optimistic" and "Pessimistic" bounds.
*   **p50**: The Median (Expected Outcome).
*   Report **Standard Deviation** to quantify the "Volatility" of the forecast.
