const report = document.getElementById('test-report');

function logResult(name, passed, errorMsg = "") {
    const div = document.createElement('div');
    div.className = passed ? "pass" : "fail";
    div.innerHTML = `[${passed ? "PASS" : "FAIL"}] ${name} ${errorMsg ? `<br>&nbsp;&nbsp;↳ ${errorMsg}` : ""}`;
    report.appendChild(div);
}

function expect(actual) {
    return {
        toBe: (expected) => {
            if (actual !== expected) throw new Error(`Expected ${expected}, but got ${actual}`);
        },
        toContain: (expected) => {
            if (!actual.includes(expected)) throw new Error(`Expected text to contain ${expected}, but got ${actual}`);
        }
    };
}

async function runTests() {
    const deck = window.klako;
    if (!deck) {
        logResult("Init", false, "window.klako is undefined");
        return;
    }

    try {
        // Test 1: Theme Toggle
        document.documentElement.setAttribute('data-theme', 'dark');
        document.getElementById('theme-toggle').click();
        expect(document.documentElement.getAttribute('data-theme')).toBe('light');
        document.getElementById('theme-toggle').click();
        expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
        logResult("Theme Toggle correctly flips polarity", true);
    } catch (e) {
        logResult("Theme Toggle correctly flips polarity", false, e.message);
    }

    try {
        // Test 2: Narrative Appending with Personas
        deck.routeEvent({ type: "NarrativeDelta", role: "thinker", tier: "L0_Thinker", text: "Hello" });
        deck.routeEvent({ type: "NarrativeDelta", role: "thinker", tier: "L0_Thinker", text: " World!" });
        const wraps = document.querySelectorAll('.message-wrap.agent');
        expect(wraps.length > 0).toBe(true);
        const lastWrap = wraps[wraps.length - 1];
        expect(lastWrap.querySelector('.persona-label').textContent).toContain("L0_Thinker");
        expect(lastWrap.querySelector('.message').textContent).toContain("Hello World!");
        logResult("NarrativeDelta creates Persona wraps and concatenates text correctly", true);
    } catch (e) {
        logResult("NarrativeDelta creates Persona wraps and concatenates text correctly", false, e.message);
    }

    try {
        // Test 3: Canvas Buffer Capacity
        // MAX_LINES is bounded at 500
        for(let i=0; i<600; i++) {
            deck.routeEvent({ type: "CanvasTelemetry", line: `Line ${i}` });
        }
        expect(deck.telemetryBuffer.length).toBe(500);
        
        // Assert execution delta and structured HH:MM prefix was attached
        const lastLineObj = deck.telemetryBuffer[499];
        const lastLine = lastLineObj.parsedLine;
        if (!/^\[\d{2}:\d{2}\s\+\d+ms\]/.test(lastLine)) {
            throw new Error(`Line did not contain HH:MM execution delta: ${lastLine}`);
        }
        expect(lastLine).toContain("Line 599");
        logResult("Canvas Telemetry applies execution deltas and limits circular buffer", true);
    } catch (e) {
        logResult("Canvas Telemetry applies execution deltas and limits circular buffer", false, e.message);
    }
    
    // Yield to the GPU requestAnimationFrame loop so it can process the 600 line injection
    await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));

    try {
        // Test 4b: Sticky Date Header synchronization
        const canvasDateElt = document.getElementById('canvas-sticky-date');
        expect(canvasDateElt.classList.contains('visible')).toBe(true);
        expect(canvasDateElt.textContent.length > 5).toBe(true);
        const narrativeHeaders = document.querySelectorAll('.sticky-date-header');
        expect(narrativeHeaders.length > 0).toBe(true);
        logResult("Sticky Date Headers are dynamically applied to both Canvas and Narrative DOM", true);
    } catch (e) {
        logResult("Sticky Date Headers are dynamically applied to both Canvas and Narrative DOM", false, e.message);
    }

    try {
        // Test 4: Router Status changes and Body Interruption State
        deck.routeEvent({ type: "StatusUpdate", role: "thinker", tier: "L0_Thinker" });
        expect(document.getElementById('tier-indicator').className).toBe("status-indicator thinker");
        expect(document.getElementById('active-tier').textContent).toBe("L0_Thinker");
        expect(document.body.getAttribute('data-router-state')).toBe("active");
        
        deck.routeEvent({ type: "StatusUpdate", role: "idle", tier: "Idle" });
        expect(document.body.getAttribute('data-router-state')).toBe("idle");
        
        logResult("StatusUpdate shifts router UI states and controls Interruption visual pulse", true);
    } catch (e) {
        logResult("StatusUpdate shifts router UI states and controls Interruption visual pulse", false, e.message);
    }
}

// Ensure execution waits for app.js DOMContentLoaded binding
setTimeout(runTests, 150);
