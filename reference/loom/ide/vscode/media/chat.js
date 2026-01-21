// Loom Chat Webview Script
(function () {
	const vscode = acquireVsCodeApi();

	// State
	let state = {
		messages: [],
		sessions: [],
		activeSessionId: null,
		isProcessing: false,
		streamingMessageId: null,
		streamingContent: '',
		toolCalls: new Map(),
	};

	// DOM Elements
	const connectionStatus = document.getElementById('connectionStatus');
	const sessionSelect = document.getElementById('sessionSelect');
	const messagesContainer = document.getElementById('messagesContainer');
	const emptyState = document.getElementById('emptyState');
	const streamingIndicator = document.getElementById('streamingIndicator');
	const messageInput = document.getElementById('messageInput');
	const sendButton = document.getElementById('sendButton');
	const cancelButton = document.getElementById('cancelButton');

	// Initialize
	function init() {
		setupEventListeners();
		vscode.postMessage({ type: 'ready' });
	}

	function setupEventListeners() {
		sendButton.addEventListener('click', sendMessage);

		messageInput.addEventListener('keydown', (e) => {
			if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
				e.preventDefault();
				sendMessage();
			}
		});

		messageInput.addEventListener('input', () => {
			messageInput.style.height = 'auto';
			messageInput.style.height = Math.min(messageInput.scrollHeight, 150) + 'px';
		});

		cancelButton.addEventListener('click', () => {
			vscode.postMessage({ type: 'cancel' });
		});

		sessionSelect.addEventListener('change', (e) => {
			const sessionId = e.target.value;
			if (sessionId) {
				vscode.postMessage({ type: 'switchSession', sessionId });
			}
		});

		window.addEventListener('message', handleExtensionMessage);
	}

	function sendMessage() {
		const text = messageInput.value.trim();
		if (!text || state.isProcessing) return;

		vscode.postMessage({ type: 'sendMessage', text });
		messageInput.value = '';
		messageInput.style.height = 'auto';
	}

	function handleExtensionMessage(event) {
		const message = event.data;

		switch (message.type) {
			case 'connectionStatus':
				updateConnectionStatus(message.connected, message.error);
				break;

			case 'sessionChanged':
				state.activeSessionId = message.sessionId;
				state.messages = message.messages;
				renderMessages();
				break;

			case 'sessionsUpdated':
				state.sessions = message.sessions;
				state.activeSessionId = message.activeSessionId;
				updateSessionSelect();
				break;

			case 'messageAdded':
				state.messages.push(message.message);
				renderMessages();
				scrollToBottom();
				break;

			case 'streamingChunk':
				appendStreamingChunk(message.content, message.messageId);
				break;

			case 'toolCallUpdate':
				updateToolCall(message.toolCall);
				break;

			case 'turnCompleted':
				state.isProcessing = false;
				state.streamingMessageId = null;
				state.streamingContent = '';
				updateProcessingState();
				break;

			case 'conversationHistory':
				state.messages = message.messages;
				renderMessages();
				break;

			case 'error':
				showError(message.message);
				state.isProcessing = false;
				updateProcessingState();
				break;
		}
	}

	function updateConnectionStatus(connected, error) {
		const indicator = connectionStatus.querySelector('.status-indicator');
		const text = connectionStatus.querySelector('.status-text');

		indicator.className = 'status-indicator ' + (connected ? 'connected' : 'disconnected');
		text.textContent = connected ? 'Connected' : (error || 'Disconnected');

		sendButton.disabled = !connected;
	}

	function updateSessionSelect() {
		sessionSelect.innerHTML = '';

		if (state.sessions.length === 0) {
			const option = document.createElement('option');
			option.value = '';
			option.textContent = 'No sessions';
			sessionSelect.appendChild(option);
			return;
		}

		state.sessions.forEach((session) => {
			const option = document.createElement('option');
			option.value = session.id;
			option.textContent = session.name || `Session ${session.id.slice(0, 8)}`;
			option.selected = session.id === state.activeSessionId;
			sessionSelect.appendChild(option);
		});
	}

	function renderMessages() {
		if (state.messages.length === 0) {
			emptyState.style.display = 'flex';
			messagesContainer.innerHTML = '';
			messagesContainer.appendChild(emptyState);
			return;
		}

		emptyState.style.display = 'none';
		messagesContainer.innerHTML = '';

		state.messages.forEach((msg) => {
			const messageEl = renderMessage(msg);
			messagesContainer.appendChild(messageEl);
		});

		scrollToBottom();
	}

	function renderMessage(message) {
		const div = document.createElement('div');
		div.className = `message ${message.role}`;
		div.dataset.messageId = message.id;

		const contentDiv = document.createElement('div');
		contentDiv.className = 'message-content';

		message.content.forEach((block) => {
			if (block.type === 'text') {
				contentDiv.appendChild(renderTextContent(block.text));
			} else if (block.type === 'tool_use') {
				contentDiv.appendChild(renderToolCall({
					id: block.id,
					name: block.name,
					arguments: JSON.stringify(block.input, null, 2),
					status: 'pending',
				}));
			} else if (block.type === 'tool_result') {
				const toolCallEl = contentDiv.querySelector(`[data-tool-id="${block.tool_use_id}"]`);
				if (toolCallEl) {
					const resultDiv = document.createElement('div');
					resultDiv.className = 'tool-result' + (block.is_error ? ' error' : '');
					resultDiv.textContent = block.content;
					toolCallEl.appendChild(resultDiv);
				}
			}
		});

		div.appendChild(contentDiv);
		return div;
	}

	function renderTextContent(text) {
		const fragment = document.createDocumentFragment();
		const parts = text.split(/(```[\s\S]*?```)/g);

		parts.forEach((part) => {
			if (part.startsWith('```') && part.endsWith('```')) {
				const codeMatch = part.match(/```(\w*)\n?([\s\S]*?)```/);
				if (codeMatch) {
					const wrapper = document.createElement('div');
					wrapper.className = 'code-block-wrapper';

					const pre = document.createElement('pre');
					const code = document.createElement('code');
					code.textContent = codeMatch[2];
					pre.appendChild(code);

					const actions = document.createElement('div');
					actions.className = 'code-actions';

					const copyBtn = document.createElement('button');
					copyBtn.className = 'code-action-btn';
					copyBtn.textContent = 'Copy';
					copyBtn.onclick = () => {
						vscode.postMessage({ type: 'copyCode', code: codeMatch[2] });
						copyBtn.textContent = 'Copied!';
						setTimeout(() => { copyBtn.textContent = 'Copy'; }, 2000);
					};

					const insertBtn = document.createElement('button');
					insertBtn.className = 'code-action-btn';
					insertBtn.textContent = 'Insert';
					insertBtn.onclick = () => {
						vscode.postMessage({ type: 'insertCode', code: codeMatch[2] });
					};

					actions.appendChild(copyBtn);
					actions.appendChild(insertBtn);
					wrapper.appendChild(pre);
					wrapper.appendChild(actions);
					fragment.appendChild(wrapper);
				}
			} else if (part.trim()) {
				const span = document.createElement('span');
				span.innerHTML = escapeHtml(part).replace(/`([^`]+)`/g, '<code>$1</code>');
				fragment.appendChild(span);
			}
		});

		return fragment;
	}

	function renderToolCall(toolCall) {
		const div = document.createElement('div');
		div.className = 'tool-call';
		div.dataset.toolId = toolCall.id;

		const header = document.createElement('div');
		header.className = 'tool-call-header';

		const icon = document.createElement('span');
		icon.className = 'tool-icon';
		icon.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16"><path fill="currentColor" d="M14.773 3.485l-.78-.781a.5.5 0 0 0-.707 0L6.586 9.404l-2.879-2.879a.5.5 0 0 0-.707 0l-.78.78a.5.5 0 0 0 0 .707l4.012 4.012a.5.5 0 0 0 .707 0l7.834-7.832a.5.5 0 0 0 0-.707z"/></svg>';

		const name = document.createElement('span');
		name.className = 'tool-name';
		name.textContent = toolCall.name;

		const status = document.createElement('span');
		status.className = 'tool-status ' + toolCall.status;
		status.textContent = toolCall.status;

		header.appendChild(icon);
		header.appendChild(name);
		header.appendChild(status);
		div.appendChild(header);

		if (toolCall.arguments) {
			const args = document.createElement('div');
			args.className = 'tool-arguments';
			args.textContent = toolCall.arguments;
			div.appendChild(args);
		}

		if (toolCall.result) {
			const result = document.createElement('div');
			result.className = 'tool-result' + (toolCall.isError ? ' error' : '');
			result.textContent = toolCall.result;
			div.appendChild(result);
		}

		return div;
	}

	function appendStreamingChunk(content, messageId) {
		if (state.streamingMessageId !== messageId) {
			state.streamingMessageId = messageId;
			state.streamingContent = '';
			state.isProcessing = true;
			updateProcessingState();
		}

		state.streamingContent += content;

		let messageEl = messagesContainer.querySelector(`[data-message-id="${messageId}"]`);
		if (!messageEl) {
			messageEl = document.createElement('div');
			messageEl.className = 'message assistant';
			messageEl.dataset.messageId = messageId;

			const contentDiv = document.createElement('div');
			contentDiv.className = 'message-content';
			messageEl.appendChild(contentDiv);

			messagesContainer.appendChild(messageEl);
			emptyState.style.display = 'none';
		}

		const contentDiv = messageEl.querySelector('.message-content');
		contentDiv.innerHTML = '';
		contentDiv.appendChild(renderTextContent(state.streamingContent));

		scrollToBottom();
	}

	function updateToolCall(toolCall) {
		state.toolCalls.set(toolCall.id, toolCall);

		const toolEl = document.querySelector(`[data-tool-id="${toolCall.id}"]`);
		if (toolEl) {
			const status = toolEl.querySelector('.tool-status');
			if (status) {
				status.className = 'tool-status ' + toolCall.status;
				status.textContent = toolCall.status;
			}

			if (toolCall.arguments) {
				let argsEl = toolEl.querySelector('.tool-arguments');
				if (!argsEl) {
					argsEl = document.createElement('div');
					argsEl.className = 'tool-arguments';
					toolEl.appendChild(argsEl);
				}
				argsEl.textContent = toolCall.arguments;
			}

			if (toolCall.result) {
				let resultEl = toolEl.querySelector('.tool-result');
				if (!resultEl) {
					resultEl = document.createElement('div');
					resultEl.className = 'tool-result';
					toolEl.appendChild(resultEl);
				}
				resultEl.className = 'tool-result' + (toolCall.isError ? ' error' : '');
				resultEl.textContent = toolCall.result;
			}
		}
	}

	function updateProcessingState() {
		streamingIndicator.style.display = state.isProcessing ? 'flex' : 'none';
		cancelButton.style.display = state.isProcessing ? 'block' : 'none';
		sendButton.disabled = state.isProcessing;
		messageInput.disabled = state.isProcessing;
	}

	function showError(message) {
		const errorDiv = document.createElement('div');
		errorDiv.className = 'error-message';
		errorDiv.textContent = message;
		messagesContainer.appendChild(errorDiv);
		scrollToBottom();

		setTimeout(() => {
			errorDiv.remove();
		}, 5000);
	}

	function scrollToBottom() {
		messagesContainer.scrollTop = messagesContainer.scrollHeight;
	}

	function escapeHtml(text) {
		const div = document.createElement('div');
		div.textContent = text;
		return div.innerHTML;
	}

	init();
})();
