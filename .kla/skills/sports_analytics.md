# Skill: Sports Analytics (Basketball)

You are equipped with the Sports Analytics skill block. You MUST adhere to the following when simulating games:

## 1. Do Not Hardcode Team Strengths
Never estimate KenPom or advanced efficiencies. If you must run a simulation without fetching external data, explicitly load testing datasets or define transparently that you are using mocked, random variables, and print a warning to the user.

## 2. Model the Possession Game Correctly
*   Basketball is a possession-based sport. Modeling should occur at the discrete possession level.
*   **Pace:** Calculate total possessions per team by using the formula: $BasePace + \text{Variance}$.
*   **Efficiency distributions:** Derive true shooting percentages from Beta distributions instead of applying a raw multiplier.

## 3. Advanced Game State (Path Dependence)
*   **Fouls & Bonus State:** Track fouls explicitly. Teams in the bonus have distinct possession outcomes (increased Free Throw probability). This is a vital Markov state.
*   **Clutch Time Modifier:** If score differential is $\leq 5$ with $\leq 5$ minutes left, the variation of possessions changes (e.g. pace decreases, variance of outcome increases).

## 4. Execution vs Output
*   **DO NOT** output raw markdown code blocks. As the Thinker, you must explicitly delegate execution by using the `Agent` tool to summon `L0_typist` (or an appropriate execution agent), handing it a detailed plan to write the simulation script via `write_file` and execute it via `bash`. Do not attempt to run tools you lack by hallucinating them.

## 5. Visualizing Results
*   Always structure the results of a simulation to be consumed as a visual artifact.
*   Print win probability.
*   Generate a `matplotlib` graphic (e.g. `sports_sim_results.png`) displaying the score differential distribution across all $N$ Monte Carlo paths.
