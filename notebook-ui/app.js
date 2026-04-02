class MockStreamGenerator {
    constructor(deck) {
        this.deck = deck;
        this.interval = null;
        this.messages = [
            "[System: Router Initialized]",
            "[L0_Thinker] Analyzing prompt context...",
            "[L0_Thinker] Generating structured execution plan.",
            "[Router] Escaping to L0_Typist due to 'bash' requirement.",
            "[L0_Typist] Executing: cargo check",
            "   Compiling api v0.1.0",
            "   Compiling kla-cli v0.1.0",
            "    Finished dev [unoptimized + debuginfo] target(s) in 0.43s",
            "[System: Execution complete]",
            "Waiting for next sequence..."
        ];
    }

    start() {
        let i = 0;
        this.interval = setInterval(() => {
            if (i >= this.messages.length) {
                i = 0;
                document.body.setAttribute('data-router-state', 'idle');
                this.deck.routeEvent({
                    type: "StatusUpdate",
                    role: "idle",
                    tier: "L0_Thinker // Idle"
                });
            } else {
                document.body.setAttribute('data-router-state', 'active');
                const msg = this.messages[i];
                if (msg.includes("L0_Thinker")) {
                    this.deck.routeEvent({ type: "StatusUpdate", role: "thinker", tier: "L0_Thinker // Reasoning" });
                } else if (msg.includes("L0_Typist")) {
                    this.deck.routeEvent({ type: "StatusUpdate", role: "typist", tier: "L0_Typist // Executing" });
                }
                
                this.deck.routeEvent({
                    type: "CanvasTelemetry",
                    line: msg
                });
                
                // Randomly generate some narrative text
                if (Math.random() > 0.7 && msg.includes("L0_Thinker")) {
                    this.deck.routeEvent({
                        type: "NarrativeDelta",
                        text: "I am analyzing the current cargo workspace structure to ensure we have no cyclic dependencies. "
                    });
                }
            }
            i++;
        }, 800);
    }
}

class KlakoFlightDeck {
    constructor() {
        this.canvas = document.getElementById('telemetry-canvas');
        this.ctx = this.canvas.getContext('2d', { alpha: false }); 
        this.history = document.getElementById('chat-history');
        this.indicator = document.getElementById('tier-indicator');
        this.activeTier = document.getElementById('active-tier');
        this.input = document.getElementById('prompt-input');
        
        this.telemetryBuffer = [];
        this.MAX_LINES = 500;
        this.currentNarrativeBubble = null;
        this.currentTier = null;
        this.lastTelemetryStamp = performance.now();
        this.lastNarrativeDateStr = null;
        
        this.initTheme();
        this.initCanvas();
        this.bindEvents();
        this.startGPULoop();
        
        const urlParams = new URLSearchParams(window.location.search);
        if (urlParams.get('mock') === '1') {
            console.log("Starting Mock Generator...");
            this.mock = new MockStreamGenerator(this);
            this.mock.start();
        } else {
            this.connectWebSocket();
        }
    }

    connectWebSocket() {
        this.ws = new WebSocket('ws://localhost:3000/stream');
        this.ws.onmessage = (event) => {
            try {
                const payload = JSON.parse(event.data);
                this.routeEvent(payload);
            } catch (e) {
                console.error("Invalid WS payload", e);
            }
        };
        this.ws.onopen = () => {
             this.routeEvent({ type: "CanvasTelemetry", line: "[System] Connected to Rust Kernel via WebSocket." });
        };
        this.ws.onclose = () => {
             this.routeEvent({ type: "CanvasTelemetry", line: "[System] WebSocket connection lost. Reconnecting in 3s..." });
             setTimeout(() => this.connectWebSocket(), 3000);
        };
    }

    initTheme() {
        const savedTheme = localStorage.getItem('klako-theme') || 'dark';
        document.documentElement.setAttribute('data-theme', savedTheme);
        
        document.getElementById('theme-toggle').addEventListener('click', () => {
            const current = document.documentElement.getAttribute('data-theme');
            const next = current === 'dark' ? 'light' : 'dark';
            document.documentElement.setAttribute('data-theme', next);
            localStorage.setItem('klako-theme', next);
        });
    }

    initCanvas() {
        this.canvasContainer = document.getElementById('canvas-container');
        this.canvasContainer.style.overflowY = 'auto'; // Native scrolling
        
        const dpr = window.devicePixelRatio || 1;
        const resizeHandle = () => {
            const rect = this.canvasContainer.getBoundingClientRect();
            
            // Virtual Height = max(container height, total lines * lineHeight) + padding
            const virtualHeight = Math.max(rect.height, this.telemetryBuffer.length * 20 + 48);
            
            this.canvas.width = rect.width * dpr;
            this.canvas.height = virtualHeight * dpr;
            this.ctx.scale(dpr, dpr);
            
            this.canvas.style.width = `${rect.width}px`;
            this.canvas.style.height = `${virtualHeight}px`;
        };
        window.addEventListener('resize', resizeHandle);
        this.resizeHandle = resizeHandle;
        resizeHandle();
    }

    bindEvents() {
        this.input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                const text = this.input.value.trim();
                if (text) {
                    this.appendUserMessage(text);
                    this.input.value = '';
                    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                        this.ws.send(JSON.stringify({ type: "SubmitPrompt", text }));
                    } else if (this.mock) {
                         this.routeEvent({ type: "CanvasTelemetry", line: `[Input] User dispatched: ${text}` });
                    }
                }
            }
        });
    }
    
    appendUserMessage(text) {
        document.body.setAttribute('data-router-state', 'active');
        
        const d = new Date();
        const dateStr = d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
        this.checkNarrativeDateShift(dateStr);

        const bubbleWrap = document.createElement('div');
        bubbleWrap.className = "message-wrap user";
        
        const bubble = document.createElement('div');
        bubble.className = "message user";
        bubble.textContent = text;
        
        bubbleWrap.appendChild(bubble);
        this.history.appendChild(bubbleWrap);
        
        // Inject typing indicator
        const typingWrap = document.createElement('div');
        typingWrap.className = 'message-wrap agent thinker typing-indicator-wrap';
        const typingLabel = document.createElement('div');
        typingLabel.className = 'persona-label';
        typingLabel.textContent = `[ Awaiting Telemetry ]`;
        const typingBubble = document.createElement('div');
        typingBubble.className = 'message agent typing-indicator';
        typingBubble.innerHTML = '<span>.</span><span>.</span><span>.</span>';
        typingWrap.appendChild(typingLabel);
        typingWrap.appendChild(typingBubble);
        this.history.appendChild(typingWrap);
        
        this.scrollToBottom();
        this.currentNarrativeBubble = null; 
        this.currentNarrativeText = '';
        this.currentTier = null;
    }

    routeEvent(payload) {
        switch (payload.type) {
            case "StatusUpdate":
                this.indicator.className = `status-indicator ${payload.role}`;
                this.activeTier.textContent = payload.tier;
                if (payload.role === "idle") {
                    document.body.setAttribute('data-router-state', 'idle');
                } else {
                    document.body.setAttribute('data-router-state', 'active');
                }
                break;
            case "NarrativeDelta":
                const d = new Date();
                const dateStr = d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
                this.checkNarrativeDateShift(dateStr);

                if (!this.currentNarrativeBubble || this.currentTier !== payload.tier) {
                    this.currentTier = payload.tier || "L0_Thinker";
                    
                    // Remove typing indicator if present
                    const typingWrap = document.querySelector('.typing-indicator-wrap');
                    if (typingWrap) {
                        typingWrap.remove();
                    }
                    
                    const bubbleWrap = document.createElement('div');
                    bubbleWrap.className = `message-wrap agent ${payload.role || "thinker"}`;
                    
                    const label = document.createElement('div');
                    label.className = "persona-label";
                    label.textContent = `[ ${this.currentTier} ]`;
                    
                    const bubble = document.createElement('div');
                    bubble.className = "message agent markup-content";
                    
                    bubbleWrap.appendChild(label);
                    bubbleWrap.appendChild(bubble);
                    this.history.appendChild(bubbleWrap);
                    
                    this.currentNarrativeBubble = bubble;
                    this.currentNarrativeText = '';
                }
                this.currentNarrativeText += payload.text;
                
                // Securely render markdown
                const rawHTML = marked.parse(this.currentNarrativeText);
                const safeHTML = DOMPurify.sanitize(rawHTML);
                this.currentNarrativeBubble.innerHTML = safeHTML;
                
                // Apply syntax highlighting
                this.currentNarrativeBubble.querySelectorAll('pre code').forEach((block) => {
                    hljs.highlightElement(block);
                });
                
                this.scrollToBottom();
                break;
            case "CanvasTelemetry":
                const now = performance.now();
                const deltaMs = Math.round(now - this.lastTelemetryStamp);
                this.lastTelemetryStamp = now;
                
                const d2 = new Date();
                const dateStr2 = d2.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
                const timeStr = d2.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit' });
                
                this.telemetryBuffer.push({
                    dateFormatted: dateStr2,
                    parsedLine: `[${timeStr} +${deltaMs}ms] ${payload.line}`
                });
                
                if (this.telemetryBuffer.length > this.MAX_LINES) {
                    this.telemetryBuffer.shift();
                }
                this.resizeHandle(); // Stretch canvas dynamically
                
                // Auto-scroll mechanics to bottom if clamped
                const isAtBottom = this.canvasContainer.scrollHeight - this.canvasContainer.clientHeight <= this.canvasContainer.scrollTop + 40;
                if (isAtBottom || this.telemetryBuffer.length < this.MAX_LINES) {
                     this.canvasContainer.scrollTop = this.canvasContainer.scrollHeight;
                }
                break;
            case "PermissionRequest":
                const modal = document.getElementById('permission-modal');
                document.getElementById('modal-tool-name').textContent = `Execute Tool: ${payload.tool}`;
                document.getElementById('modal-tool-desc').textContent = `Target execution mode: ${payload.required_mode}`;
                document.getElementById('modal-tool-input').textContent = payload.input;
                modal.classList.remove('hidden');
                
                const allowBtn = document.getElementById('modal-allow-btn');
                const denyBtn = document.getElementById('modal-deny-btn');
                
                const handleDecision = (decision) => {
                    modal.classList.add('hidden');
                    allowBtn.onclick = null;
                    denyBtn.onclick = null;
                    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                        this.ws.send(JSON.stringify({ type: "PermissionResponse", decision }));
                    }
                };
                
                allowBtn.onclick = () => handleDecision("allow");
                denyBtn.onclick = () => handleDecision("deny");
                break;
        }
    }
    
    checkNarrativeDateShift(newDateStr) {
        if (this.lastNarrativeDateStr !== newDateStr) {
            this.lastNarrativeDateStr = newDateStr;
            const header = document.createElement('div');
            header.className = "sticky-date-header";
            header.textContent = newDateStr;
            this.history.appendChild(header);
            this.currentNarrativeBubble = null; // force break
            this.currentTier = null;
        }
    }
    
    scrollToBottom() {
        this.history.scrollTop = this.history.scrollHeight;
    }

    startGPULoop() {
        const render = () => {
            const isLight = document.documentElement.getAttribute('data-theme') === 'light';
            
            this.ctx.fillStyle = isLight ? '#F9FAFB' : '#0A0A0B';
            this.ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
            
            this.ctx.fillStyle = isLight ? '#0A0A0B' : '#FFFFFF';
            this.ctx.font = '13px "Fira Code", monospace';
            
            const lineHeight = 20;
            const padding = 24;
            
            const scrollTop = this.canvasContainer.scrollTop;
            const viewportHeight = this.canvasContainer.clientHeight;
            const startIdx = Math.max(0, Math.floor((scrollTop - padding) / lineHeight));
            const endIdx = Math.min(this.telemetryBuffer.length, Math.ceil((scrollTop + viewportHeight - padding) / lineHeight));
            
            const visibleBuffer = this.telemetryBuffer.slice(startIdx, endIdx);
            
            let y = padding + (startIdx * lineHeight);
            
            for (let i = 0; i < visibleBuffer.length; i++) {
                this.ctx.fillText(visibleBuffer[i].parsedLine, padding, y);
                y += lineHeight;
            }
            
            // Sync Canvas Sticky Date Overlay
            const canvasDateElt = document.getElementById('canvas-sticky-date');
            if (visibleBuffer.length > 0) {
                const topDate = visibleBuffer[0].dateFormatted;
                if (topDate && canvasDateElt.textContent !== topDate) {
                    canvasDateElt.textContent = topDate;
                    canvasDateElt.classList.add('visible');
                }
            } else {
                canvasDateElt.classList.remove('visible');
            }
            
            requestAnimationFrame(render);
        };
        requestAnimationFrame(render);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    window.klako = new KlakoFlightDeck();
});
