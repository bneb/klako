# Skill: Sports Analytics (High-Fidelity Worlds)

You are equipped with the **SportsWorld** simulation kernel. You MUST adhere to the following when simulating games:

## 1. Engine Priority: The "World-First" Mandate
NEVER write manual Python random walks for match outcomes. You MUST route all simulations through the `SportsWorld` tool using the appropriate operation:
*   `simulate_soccer_v2`: Use for all Football/Soccer matches. You MUST provide `card_hazard` (discipline), `bench_depth` (sub impact), and `set_piece_potency` (specialist scoring).
*   `simulate_baseball_v3`: Use for all MLB/Baseball simulations. You MUST provide a full 9-man `lineup` (OBP/SLG per batter) and a `pitching_staff` (Starter + Bullpen WHIP). The engine uses PA-level discrete event simulation.
*   `simulate_baseball`: Legacy team-level simulator. Prefer V3.
*   `simulate_tennis`: Use for ATP/WTA. Operates on point-by-point fractal probability.
*   `simulate_basketball`: Use for NBA/NCAA. Operates on possession-level efficiency.

## 2. Parameter Discovery (The Scout Protocol)
Before calling a World operation, you MUST use `WebFetch` or `GoogleSearch` to retrieve:
*   **Soccer**: xG/xGA per 90, current Elo, and goalkeeper form (Z-score).
*   **Baseball**: OBP, SLG, and Pitcher WHIP for the scheduled starter.
*   **Tennis**: Service Point Win % and Return Point Win % on the specific surface (Clay/Hard/Grass).

## 3. Path Dependence & Tactical Context
Always account for the "Manager Intent" in your environment setup:
*   **Weather**: In Soccer, set `weather_intensity > 0.5` for rain/wind to model conversion drag.
*   **Fatigue**: Calculate `hours_rest` and `travel_km` to scale stamina decay.

## 4. Output Calibration
*   Compare simulation results against market odds (if available).
*   Report the **Brier Score** for all backtested historical results to prove model calibration.
*   Always generate a summary table including Win Probabilities and Projected Totals.
