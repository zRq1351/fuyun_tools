class Toast {
    constructor() {
        this.container = null;
        this.initialized = false;
        this.initQueue = [];
    }

    init() {
        if (this.initialized) return;

        if (!document.body) {
            if (document.readyState === 'loading') {
                document.addEventListener('DOMContentLoaded', () => this._init());
            } else {
                const tryInit = () => {
                    if (document.body) {
                        this._init();
                    } else {
                        requestAnimationFrame(tryInit);
                    }
                };
                tryInit();
            }
            return;
        }

        this._init();
    }

    _init() {
        if (this.initialized || !document.body) return;

        this.container = document.createElement('div');
        this.container.className = 'toast-container';
        document.body.appendChild(this.container);
        this.initialized = true;

        while (this.initQueue.length > 0) {
            const { message, type, duration } = this.initQueue.shift();
            this.show(message, type, duration);
        }
    }

    show(message, type = 'success', duration = 3000) {
        if (!this.initialized) {
            this.init();
            if (!this.initialized) {
                this.initQueue.push({ message, type, duration });
                return null;
            }
        }

        if (!this.container || !document.body.contains(this.container)) {
            this._init();
        }

        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;

        const icon = document.createElement('i');
        icon.className = this.getIconClass(type);

        const messageSpan = document.createElement('span');
        messageSpan.className = 'toast-message';
        messageSpan.textContent = message;

        const closeBtn = document.createElement('button');
        closeBtn.className = 'toast-close';
        closeBtn.innerHTML = '&times;';
        closeBtn.setAttribute('aria-label', '关闭提示');
        closeBtn.onclick = () => this.hide(toast);

        toast.appendChild(icon);
        toast.appendChild(messageSpan);
        toast.appendChild(closeBtn);

        this.container.appendChild(toast);

        if (duration > 0) {
            setTimeout(() => this.hide(toast), duration);
        }

        return toast;
    }

    hide(toast) {
        if (!toast || !toast.parentNode) return;

        toast.style.animation = 'slideOut 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55) forwards';
        setTimeout(() => {
            if (toast.parentNode === this.container) {
                this.container.removeChild(toast);
            }
        }, 300);
    }

    getIconClass(type) {
        const icons = {
            success: 'fas fa-check-circle',
            error: 'fas fa-exclamation-circle',
            warning: 'fas fa-exclamation-triangle',
            info: 'fas fa-info-circle'
        };
        return icons[type] || icons.info;
    }

    success(message, duration = 3000) {
        return this.show(message, 'success', duration);
    }

    error(message, duration = 3000) {
        return this.show(message, 'error', duration);
    }

    warning(message, duration = 3000) {
        return this.show(message, 'warning', duration);
    }

    info(message, duration = 3000) {
        return this.show(message, 'info', duration);
    }
}

let toastInstance = null;

function getToastInstance() {
    if (!toastInstance) {
        toastInstance = new Toast();
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', () => toastInstance.init());
        } else {
            toastInstance.init();
        }
    }
    return toastInstance;
}

window.showToast = (message, type = 'success', duration = 3000) =>
    getToastInstance().show(message, type, duration);

window.showSuccessToast = (message, duration = 3000) =>
    getToastInstance().success(message, duration);

window.showErrorToast = (message, duration = 3000) =>
    getToastInstance().error(message, duration);

window.showWarningToast = (message, duration = 3000) =>
    getToastInstance().warning(message, duration);

window.showInfoToast = (message, duration = 3000) =>
    getToastInstance().info(message, duration);

window.Toast = {
    show: window.showToast,
    success: window.showSuccessToast,
    error: window.showErrorToast,
    warning: window.showWarningToast,
    info: window.showInfoToast,
    init: () => getToastInstance().init()
};