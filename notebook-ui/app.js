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
        this.ctx = this.canvas.getContext('2d'); 
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
        this.renderQueuePending = false;
        this.subagentCards = new Map();
        this.subagentGrid = document.getElementById('subagent-grid');
        
        this.initTheme();
        this.initCanvas();
        this.bindEvents();
        this.bindPaneToggles();
        this.bindCopyUtilities();
        this.bindReviewInteractions();
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

    renderMockSprintBoard() {
        const mockLedger = {
            "sprint_id": "alpha-v1",
            "tasks": [
                { "id": "TASK-01", "title": "Implement Login API", "status": "IN_PROGRESS", "assignee": "Session-A" },
                { "id": "TASK-02", "title": "Build React Login Component", "status": "OPEN", "assignee": null },
                { "id": "TASK-03", "title": "Setup Postgres Schema", "status": "COMPLETED", "assignee": "Session-B" },
                { "id": "TASK-04", "title": "Configure OAuth via Google", "status": "OPEN", "assignee": null }
            ]
        };
        
        const openCol = document.querySelector('#col-open .task-list');
        const progCol = document.querySelector('#col-in-progress .task-list');
        const doneCol = document.querySelector('#col-done .task-list');
        
        if (!openCol || !progCol || !doneCol) return;
        
        openCol.innerHTML = '';
        progCol.innerHTML = '';
        doneCol.innerHTML = '';
        
        mockLedger.tasks.forEach(task => {
            const card = document.createElement('div');
            card.className = 'task-card';
            
            let assigneeHtml = '';
            if (task.assignee) {
                const isActive = task.status === 'IN_PROGRESS' ? 'active' : '';
                assigneeHtml = `<span class="task-assignee ${isActive}">${task.assignee}</span>`;
            }
            
            card.innerHTML = `
                <span class="task-id">${task.id}</span>
                <span class="task-title">${task.title}</span>
                ${assigneeHtml}
            `;
            
            if (task.status === 'OPEN') openCol.appendChild(card);
            else if (task.status === 'IN_PROGRESS') progCol.appendChild(card);
            else if (task.status === 'COMPLETED') doneCol.appendChild(card);
        });
    }

    connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const host = window.location.host;
        this.ws = new WebSocket(`${protocol}//${host}/stream`);
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
        const systemPrefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const defaultTheme = systemPrefersDark ? 'dark' : 'light';
        const savedTheme = localStorage.getItem('klako-theme');
        const activeTheme = savedTheme || defaultTheme;
        
        document.documentElement.setAttribute('data-theme', activeTheme);

        const themeToggle = document.getElementById('theme-toggle');
        if (themeToggle) {
            themeToggle.addEventListener('click', () => {
                const current = document.documentElement.getAttribute('data-theme');
                const next = current === 'dark' ? 'light' : 'dark';
                document.documentElement.setAttribute('data-theme', next);
                localStorage.setItem('klako-theme', next);
            });
        }

        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', event => {
            if (!localStorage.getItem('klako-theme')) {
                const newTheme = event.matches ? 'dark' : 'light';
                document.documentElement.setAttribute('data-theme', newTheme);
            }
        });
    }

    initCanvas() {
        this.canvasContainer = document.getElementById('canvas-container');
        this.canvasContainer.style.overflowY = 'auto'; // Native scrolling
        
        const dpr = window.devicePixelRatio || 1;
        const resizeHandle = () => {
            const rect = this.canvasContainer.getBoundingClientRect();
            
            // Virtual Height = max(container height, total lines * lineHeight) + padding
            const virtualHeight = Math.max(rect.height, this.telemetryBuffer.length * 20 + 80);
            
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

    bindPaneToggles() {
        const chassis = document.getElementById('klako-chassis');
        const toggleMechanics = document.getElementById('toggle-mechanics');
        const toggleSteerable = document.getElementById('toggle-steerable');
        const toggleSprint = document.getElementById('toggle-sprint');
        const toggleReview = document.getElementById('toggle-review');
        const toggleSwarm = document.getElementById('toggle-swarm');
        const mechanicsPane = document.getElementById('mechanics-pane');
        const steerablePane = document.getElementById('steerable-pane');
        const sprintPane = document.getElementById('sprint-pane');
        const reviewPane = document.getElementById('review-pane');

        const updateGridLayout = () => {
            let activeCount = 1; // Narrative pane always open
            if (mechanicsPane && mechanicsPane.classList.contains('visible')) activeCount++;
            if (steerablePane && steerablePane.classList.contains('visible')) activeCount++;
            if (sprintPane && sprintPane.classList.contains('visible')) activeCount++;
            if (reviewPane && reviewPane.classList.contains('visible')) activeCount++;
            
            if (activeCount >= 3) {
                chassis.classList.add('grid-2x2');
            } else {
                chassis.classList.remove('grid-2x2');
            }
            
            if (toggleSwarm) {
                if (activeCount >= 4 && !reviewPane.classList.contains('visible')) {
                    toggleSwarm.classList.add('active');
                } else {
                    toggleSwarm.classList.remove('active');
                }
            }
        };

        const togglePane = (paneToShow, triggerBtn, forceState = null) => {
            let isClosing = paneToShow.classList.contains('visible');
            if (forceState !== null) {
                isClosing = !forceState;
            }
            
            if (isClosing) {
                paneToShow.classList.remove('visible');
                if (triggerBtn) triggerBtn.classList.remove('active');
            } else {
                paneToShow.classList.add('visible');
                if (triggerBtn) triggerBtn.classList.add('active');
            }
            updateGridLayout();
        };

        if (toggleMechanics) toggleMechanics.addEventListener('click', () => togglePane(mechanicsPane, toggleMechanics));
        if (toggleSteerable) toggleSteerable.addEventListener('click', () => togglePane(steerablePane, toggleSteerable));
        if (toggleReview) toggleReview.addEventListener('click', () => togglePane(reviewPane, toggleReview));
        if (toggleSprint) {
            toggleSprint.addEventListener('click', () => {
                togglePane(sprintPane, toggleSprint);
                if (sprintPane.classList.contains('visible')) {
                    this.renderMockSprintBoard();
                }
            });
        }
        
        if (toggleSwarm) {
            toggleSwarm.addEventListener('click', () => {
                const isActive = toggleSwarm.classList.contains('active');
                const nextState = !isActive;
                togglePane(mechanicsPane, toggleMechanics, nextState);
                togglePane(steerablePane, toggleSteerable, nextState);
                togglePane(sprintPane, toggleSprint, nextState);
                if (nextState) {
                    this.renderMockSprintBoard();
                }
            });
        }

        const closeBtns = document.querySelectorAll('.close-pane-btn');
        closeBtns.forEach(btn => {
            btn.addEventListener('click', (e) => {
                const targetId = e.currentTarget.getAttribute('data-target');
                const pane = document.getElementById(targetId);
                if (pane) pane.classList.remove('visible');
                
                if (targetId === 'mechanics-pane' && toggleMechanics) toggleMechanics.classList.remove('active');
                if (targetId === 'steerable-pane' && toggleSteerable) toggleSteerable.classList.remove('active');
                if (targetId === 'sprint-pane' && toggleSprint) toggleSprint.classList.remove('active');
                if (targetId === 'review-pane' && toggleReview) toggleReview.classList.remove('active');
                updateGridLayout();
            });
        });
    }

    bindCopyUtilities() {
        const copyBtn = document.getElementById('copy-plan-btn');
        const steerableContent = document.getElementById('steerable-content');
        
        if (copyBtn && steerableContent) {
            copyBtn.addEventListener('click', () => {
                const rawText = steerableContent.innerText || '';
                const lines = rawText.split('\n');
                
                let output = rawText;
                if (lines.length > 250) {
                    output = lines.slice(0, 250).join('\n') + '\n\n...[Truncated: Exceeded 250 lines]...';
                }
                
                navigator.clipboard.writeText(output).then(() => {
                    const originalText = copyBtn.innerText;
                    copyBtn.innerText = 'Copied!';
                    copyBtn.style.color = 'var(--status-typist)';
                    setTimeout(() => {
                        copyBtn.innerText = originalText;
                        copyBtn.style.color = '';
                    }, 2000);
                }).catch(err => {
                    console.error("Failed to copy", err);
                });
            });
        }
    }

    bindReviewInteractions() {
        const reviewContent = document.getElementById('review-content');
        const popup = document.getElementById('review-selection-popup');
        const discussBtn = document.getElementById('discuss-selection-btn');
        
        if (!reviewContent || !popup || !discussBtn) return;
        
        let currentSelection = '';
        
        reviewContent.addEventListener('mouseup', (e) => {
            const selection = window.getSelection();
            const text = selection.toString().trim();
            
            if (text.length > 0) {
                currentSelection = text;
                const range = selection.getRangeAt(0);
                const rect = range.getBoundingClientRect();
                
                // Position popup above selection
                popup.style.left = `${rect.left + window.scrollX}px`;
                popup.style.top = `${rect.top + window.scrollY - popup.offsetHeight - 10}px`;
                popup.classList.remove('hidden');
            } else {
                popup.classList.add('hidden');
                currentSelection = '';
            }
        });
        
        // Hide on mousedown if clicking outside popup
        document.addEventListener('mousedown', (e) => {
            if (!popup.contains(e.target)) {
                popup.classList.add('hidden');
            }
        });
        
        discussBtn.addEventListener('click', () => {
            if (currentSelection) {
                const promptText = `I have a question about this section of the design document:\n\n> ${currentSelection}\n\nCan you review this using SymbolWorld and give me your thoughts?`;
                this.appendUserMessage(promptText);
                if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                    this.ws.send(JSON.stringify({ type: "SubmitPrompt", text: promptText }));
                }
                popup.classList.add('hidden');
                window.getSelection().removeAllRanges();
            }
        });
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
            case "PlanDelta":
                const steerablePane = document.getElementById('steerable-pane');
                const steerableContent = document.getElementById('steerable-content');
                if (steerableContent && steerablePane) {
                    const safePlan = DOMPurify.sanitize(marked.parse(payload.payload));
                    steerableContent.innerHTML += `<div class="plan-entry">${safePlan}</div>`;
                    
                    steerableContent.querySelectorAll('pre code').forEach((block) => {
                        hljs.highlightElement(block);
                    });

                    // Force open the pane
                    if (!steerablePane.classList.contains('visible')) {
                        const toggleBtn = document.getElementById('toggle-steerable');
                        if (toggleBtn) toggleBtn.click();
                    }
                }
                break;
            case "OpenReviewPane":
                const reviewPane = document.getElementById('review-pane');
                const reviewContent = document.getElementById('review-content');
                const reviewDocTitle = document.getElementById('review-doc-title');
                
                if (reviewPane && reviewContent) {
                    if (reviewDocTitle) {
                        reviewDocTitle.textContent = `Review: ${payload.file_path}`;
                    }
                    reviewContent.innerHTML = DOMPurify.sanitize(marked.parse(payload.content || "*Empty document*"));
                    
                    reviewContent.querySelectorAll('pre code').forEach((block) => {
                        hljs.highlightElement(block);
                    });

                    // Force open the pane
                    if (!reviewPane.classList.contains('visible')) {
                        const toggleBtn = document.getElementById('toggle-review');
                        if (toggleBtn) toggleBtn.click();
                    }
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
                
                if (!this.renderQueuePending) {
                    this.renderQueuePending = true;
                    requestAnimationFrame(() => {
                        this.renderQueuePending = false;
                        if (!this.currentNarrativeBubble) return;

                        // Securely render markdown but allow iframes for embedded webpages
                        const rawHTML = marked.parse(this.currentNarrativeText);
                        const safeHTML = DOMPurify.sanitize(rawHTML, {
                            ADD_TAGS: ['iframe'],
                            ADD_ATTR: ['allow', 'allowfullscreen', 'frameborder', 'scrolling', 'sandbox']
                        });
                        this.currentNarrativeBubble.innerHTML = safeHTML;
                        
                        // Apply syntax highlighting and render inline SVGs
                        this.currentNarrativeBubble.querySelectorAll('pre code').forEach((block) => {
                            hljs.highlightElement(block);
                            
                            const codeText = block.textContent.trim();
                            const lowerCode = codeText.toLowerCase();
                            const svgStart = lowerCode.indexOf('<svg');
                            const svgEnd = lowerCode.lastIndexOf('</svg>');
                            
                            // Robustly check if the block contains an SVG definition
                            if (svgStart !== -1 && svgEnd !== -1) {
                                let preview = block.parentElement.nextElementSibling;
                                if (!preview || !preview.classList.contains('svg-preview-container')) {
                                    preview = document.createElement('div');
                                    preview.className = 'svg-preview-container';
                                    block.parentElement.insertAdjacentElement('afterend', preview);
                                }
                                
                                // Extract only the SVG portion to prevent XML Prologues from breaking DOMPurify
                                const rawSvg = codeText.substring(svgStart, svgEnd + 6);
                                
                                // Render the SVG securely
                                preview.innerHTML = DOMPurify.sanitize(rawSvg, { USE_PROFILES: { svg: true, svgFilters: true } });
                            }
                        });
                        
                        this.scrollToBottom();
                    });
                }
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

            // ── Sub-Agent Task Card Events ──────────────────────────
            case "SubAgentStart":
                this.createSubAgentCard(payload);
                break;
            case "SubAgentDelta":
                this.appendSubAgentDelta(payload.agent_id, payload.text);
                break;
            case "SubAgentToolUse":
                this.appendSubAgentToolUse(payload.agent_id, payload.tool_name, payload.input_summary);
                break;
            case "SubAgentComplete":
                this.finalizeSubAgentCard(payload.agent_id, payload.status, payload.final_text_preview || payload.error);
                break;
            case "VisualArtifact":
                this.renderVisualArtifact(payload.artifact);
                break;
            case "SwarmLedgerUpdate":
                this.updateSprintBoard(payload);
                break;
        }
    }
    
    updateSprintBoard(ledger) {
        const openCol = document.querySelector('#col-open .task-list');
        const progCol = document.querySelector('#col-in-progress .task-list');
        const doneCol = document.querySelector('#col-done .task-list');
        
        if (!openCol || !progCol || !doneCol) return;
        
        openCol.innerHTML = '';
        progCol.innerHTML = '';
        doneCol.innerHTML = '';
        
        const agentsMap = new Map();
        if (ledger.agents) {
            ledger.agents.forEach(agent => {
                agentsMap.set(agent.id, agent);
            });
        }

        if (ledger.tasks) {
            ledger.tasks.forEach((task, index) => {
                const card = document.createElement('div');
                card.className = 'task-card';
                
                let statusClass = task.status.toLowerCase();
                if (statusClass === 'pending') statusClass = 'open';
                
                card.innerHTML = `
                    <span class="task-id">TASK-${String(index + 1).padStart(2, '0')}</span>
                    <span class="task-title">${task.description}</span>
                `;

                // If task is running, show the agent id
                if (task.status === 'Running') {
                    card.classList.add('running');
                    // Find the agent that corresponds to this task
                    // For now, we assume agents match indices if we don't have a better link
                    const agent = ledger.agents && ledger.agents[index];
                    if (agent) {
                        const active = agent.status === 'running' ? 'active' : '';
                        card.innerHTML += `<span class="task-assignee ${active}">${agent.id}</span>`;
                    }
                    progCol.appendChild(card);
                } else if (task.status === 'Completed') {
                    doneCol.appendChild(card);
                } else if (task.status === 'Pending') {
                    openCol.appendChild(card);
                } else {
                    // Failed or unknown
                    card.style.borderLeftColor = 'var(--status-thinker)'; // Red-ish
                    doneCol.appendChild(card);
                }
            });
        }
    }

    renderVisualArtifact(artifact) {
        if (!artifact) return;
        
        const bubbleWrap = document.createElement('div');
        bubbleWrap.className = "message-wrap agent visual-artifact-wrap";
        
        const label = document.createElement('div');
        label.className = "persona-label";
        label.textContent = `[ Visual System: ${artifact.type} ]`;
        
        const bubble = document.createElement('div');
        bubble.className = "message agent visual-artifact-content";
        
        if (artifact.type === 'histogram') {
            bubble.innerHTML = `<h4>${artifact.title}</h4><div class="histogram-container"></div>`;
            const container = bubble.querySelector('.histogram-container');
            // Simplified bar rendering for now
            const max = Math.max(...artifact.data);
            artifact.data.slice(0, 50).forEach(val => {
                const bar = document.createElement('div');
                bar.className = 'artifact-bar';
                bar.style.height = `${(val / max) * 100}px`;
                container.appendChild(bar);
            });
        } else if (artifact.type === 'table') {
            let html = `<h4>${artifact.title}</h4><table class="artifact-table"><thead><tr>`;
            artifact.headers.forEach(h => html += `<th>${h}</th>`);
            html += `</tr></thead><tbody>`;
            artifact.rows.forEach(row => {
                html += `<tr>`;
                row.forEach(cell => html += `<td>${cell}</td>`);
                html += `</tr>`;
            });
            html += `</tbody></table>`;
            bubble.innerHTML = html;
        } else {
            bubble.textContent = JSON.stringify(artifact, null, 2);
        }
        
        bubbleWrap.appendChild(label);
        bubbleWrap.appendChild(bubble);
        this.history.appendChild(bubbleWrap);
        this.scrollToBottom();
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
    
    // ── Sub-Agent Card Lifecycle ─────────────────────────────────

    createSubAgentCard(payload) {
        const { agent_id, name, description, model, subagent_type } = payload;

        const card = document.createElement('div');
        card.className = 'subagent-card running';
        card.id = `sa-${agent_id}`;

        const header = document.createElement('div');
        header.className = 'subagent-card-header';

        const dot = document.createElement('span');
        dot.className = 'subagent-status-dot';

        const nameEl = document.createElement('span');
        nameEl.className = 'subagent-card-name';
        nameEl.textContent = name || agent_id;

        const badge = document.createElement('span');
        badge.className = 'subagent-card-badge';
        badge.textContent = model || subagent_type || 'agent';

        header.appendChild(dot);
        header.appendChild(nameEl);
        header.appendChild(badge);

        const desc = document.createElement('div');
        desc.className = 'subagent-card-desc';
        desc.textContent = description || '';

        const output = document.createElement('div');
        output.className = 'subagent-card-output';
        output.textContent = 'Initializing…';

        const footer = document.createElement('div');
        footer.className = 'subagent-card-footer';
        footer.innerHTML = `<span>${new Date().toLocaleTimeString('en-US', { hour12: false })}</span><span>running</span>`;

        card.appendChild(header);
        card.appendChild(desc);
        card.appendChild(output);
        card.appendChild(footer);

        this.subagentGrid.appendChild(card);
        this.subagentGrid.classList.add('has-cards');

        this.subagentCards.set(agent_id, {
            card,
            output,
            footer,
            lines: [],
            maxLines: 40
        });
    }

    appendSubAgentDelta(agentId, text) {
        const entry = this.subagentCards.get(agentId);
        if (!entry) return;

        entry.lines.push(text);
        // Keep only the last N lines worth of text
        if (entry.lines.length > entry.maxLines) {
            entry.lines.shift();
        }
        entry.output.textContent = entry.lines.join('');
        // Auto-scroll the output to bottom
        entry.output.scrollTop = entry.output.scrollHeight;
    }

    appendSubAgentToolUse(agentId, toolName, inputSummary) {
        const entry = this.subagentCards.get(agentId);
        if (!entry) return;

        const toolLine = document.createElement('div');
        toolLine.className = 'subagent-tool-line';
        toolLine.textContent = `🛠️ ${toolName} · ${inputSummary || ''}`;

        // Insert before the footer
        entry.card.insertBefore(toolLine, entry.footer);
    }

    finalizeSubAgentCard(agentId, status, preview) {
        const entry = this.subagentCards.get(agentId);
        if (!entry) return;

        entry.card.classList.remove('running');
        entry.card.classList.add(status === 'completed' ? 'completed' : 'failed');

        if (preview) {
            entry.output.textContent = preview;
        }

        const badgeClass = status === 'completed' ? 'ok' : 'err';
        const badgeText = status === 'completed' ? '✓ done' : '✗ failed';
        entry.footer.innerHTML = `<span>${new Date().toLocaleTimeString('en-US', { hour12: false })}</span><span class="subagent-result-badge ${badgeClass}">${badgeText}</span>`;
    }

    scrollToBottom() {
        this.history.scrollTop = this.history.scrollHeight;
    }

    startGPULoop() {
        const render = () => {
            const isLight = document.documentElement.getAttribute('data-theme') === 'light';
            
            // Clear the canvas to be fully transparent, allowing the CSS glass background to show through
            this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
            
            this.ctx.fillStyle = isLight ? '#0A0A0B' : '#FFFFFF';
            this.ctx.font = '13px "Fira Code", monospace';
            
            const lineHeight = 20;
            const paddingY = 60; // Increased to ensure the first line cleanly drops below the 30px tall sticky date header
            
            const scrollTop = this.canvasContainer.scrollTop;
            const viewportHeight = this.canvasContainer.clientHeight;
            const startIdx = Math.max(0, Math.floor((scrollTop - paddingY) / lineHeight));
            const endIdx = Math.min(this.telemetryBuffer.length, Math.ceil((scrollTop + viewportHeight - paddingY) / lineHeight));
            
            const visibleBuffer = this.telemetryBuffer.slice(startIdx, endIdx);
            
            let y = paddingY + (startIdx * lineHeight);
            
            // Text padding horizontally (left)
            const paddingX = 24;
            
            for (let i = 0; i < visibleBuffer.length; i++) {
                this.ctx.fillText(visibleBuffer[i].parsedLine, paddingX, y);
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
