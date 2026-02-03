// toast.js - 侧滑提示消息组件
class Toast {
    constructor() {
        console.log('Toast constructor called');
        this.container = null;
        this.initialized = false;
        this.initQueue = [];
    }

    init() {
        console.log('Toast.init() called, initialized:', this.initialized, 'document.body:', !!document.body);
        if (this.initialized) return;
        
        // 使用更健壮的初始化方式，适应生产环境
        if (!document.body) {
            console.log('document.body not ready, delaying initialization');
            // 如果document.body还不存在，延迟初始化
            if (document.readyState === 'loading') {
                console.log('DOM still loading, adding DOMContentLoaded listener');
                document.addEventListener('DOMContentLoaded', () => {
                    console.log('DOMContentLoaded fired, calling _init()');
                    this._init();
                });
            } else {
                console.log('DOM loaded but body not ready, using requestAnimationFrame');
                // 使用requestAnimationFrame替代setTimeout，更适合动画
                requestAnimationFrame(() => {
                    if (!document.body) {
                        // 如果仍然没有body，使用setInterval定期检查
                        const interval = setInterval(() => {
                            if (document.body) {
                                clearInterval(interval);
                                this._init();
                            }
                        }, 10);
                    } else {
                        this._init();
                    }
                });
            }
            return;
        }
        
        this._init();
    }

    _init() {
        if (this.initialized || !document.body) return;
        
        // 创建容器
        this.container = document.createElement('div');
        this.container.className = 'toast-container';
        this.container.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            z-index: 9999;
            display: flex;
            flex-direction: column;
            gap: 10px;
            max-width: 400px;
        `;
        
        document.body.appendChild(this.container);
        this.initialized = true;
        
        // 处理队列中的消息
        while (this.initQueue.length > 0) {
            const {message, type, duration} = this.initQueue.shift();
            this.show(message, type, duration);
        }
    }

    show(message, type = 'success', duration = 3000) {
        // 确保初始化
        if (!this.initialized) {
            this.init();
            
            // 如果还没有初始化完成，将消息加入队列
            if (!this.initialized) {
                this.initQueue.push({message, type, duration});
                return null;
            }
        }
        
        // 检查容器是否存在
        if (!this.container || !document.body.contains(this.container)) {
            this._init();
        }
        
        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;
        
        // 图标
        const icon = document.createElement('i');
        icon.className = this.getIconClass(type);
        
        // 消息内容
        const messageSpan = document.createElement('span');
        messageSpan.className = 'toast-message';
        messageSpan.textContent = message;
        
        // 关闭按钮
        const closeBtn = document.createElement('button');
        closeBtn.className = 'toast-close';
        closeBtn.innerHTML = '&times;';
        closeBtn.onclick = () => this.hide(toast);
        
        toast.appendChild(icon);
        toast.appendChild(messageSpan);
        toast.appendChild(closeBtn);
        
        // 添加样式
        toast.style.cssText = `
            display: flex;
            align-items: center;
            padding: 16px 20px;
            background: ${this.getBackgroundColor(type)};
            color: ${this.getTextColor(type)};
            border-radius: 8px;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
            transform: translateX(120%);
            transition: transform 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55);
            animation: slideIn 0.3s forwards;
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            font-size: 14px;
            line-height: 1.4;
            max-width: 100%;
            box-sizing: border-box;
        `;
        
        // 图标样式
        icon.style.cssText = `
            font-size: 18px;
            margin-right: 12px;
            flex-shrink: 0;
        `;
        
        // 消息样式
        messageSpan.style.cssText = `
            flex: 1;
            margin-right: 12px;
            word-break: break-word;
        `;
        
        // 关闭按钮样式
        closeBtn.style.cssText = `
            background: none;
            border: none;
            color: inherit;
            font-size: 20px;
            cursor: pointer;
            padding: 0;
            width: 24px;
            height: 24px;
            display: flex;
            align-items: center;
            justify-content: center;
            border-radius: 4px;
            transition: background-color 0.2s;
            flex-shrink: 0;
        `;
        
        closeBtn.onmouseover = () => {
            closeBtn.style.backgroundColor = 'rgba(255, 255, 255, 0.2)';
        };
        
        closeBtn.onmouseout = () => {
            closeBtn.style.backgroundColor = 'transparent';
        };
        
        // 确保容器存在
        if (!this.container) {
            this._init();
        }
        
        this.container.appendChild(toast);
        
        // 使用内联动画样式，避免CSP限制
        const animationStyles = `
            @keyframes slideIn {
                from {
                    transform: translateX(120%);
                }
                to {
                    transform: translateX(0);
                }
            }
            
            @keyframes slideOut {
                from {
                    transform: translateX(0);
                }
                to {
                    transform: translateX(120%);
                }
            }
        `;
        
        // 检查是否已有样式
        let styleElement = document.querySelector('#toast-styles');
        if (!styleElement) {
            styleElement = document.createElement('style');
            styleElement.id = 'toast-styles';
            document.head.appendChild(styleElement);
        }
        
        // 添加动画样式
        if (styleElement && !styleElement.textContent.includes('slideIn')) {
            styleElement.textContent = animationStyles;
        }
        
        // 为当前toast添加动画类
        toast.style.animation = 'slideIn 0.3s forwards';
        
        // 自动隐藏
        if (duration > 0) {
            setTimeout(() => this.hide(toast), duration);
        }
        
        return toast;
    }

    hide(toast) {
        if (!toast) return;
        
        toast.classList.add('toast-hide');
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

    getBackgroundColor(type) {
        const colors = {
            success: '#10b981',
            error: '#ef4444',
            warning: '#f59e0b',
            info: '#3b82f6'
        };
        return colors[type] || colors.info;
    }

    getTextColor(type) {
        return '#ffffff';
    }

    getBorderColor(type) {
        const colors = {
            success: '#059669',
            error: '#dc2626',
            warning: '#d97706',
            info: '#2563eb'
        };
        return colors[type] || colors.info;
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

// 创建全局实例，但不立即初始化
let toastInstance = null;

function getToastInstance() {
    if (!toastInstance) {
        toastInstance = new Toast();
    }
    // 确保实例已初始化
    if (typeof toastInstance.init === 'function' && !toastInstance.initialized) {
        toastInstance.init();
    }
    return toastInstance;
}

// 导出函数
window.showToast = (message, type = 'success', duration = 3000) => {
    try {
        return getToastInstance().show(message, type, duration);
    } catch (e) {
        console.error('showToast error:', e);
        // 降级到简单toast
        showSimpleToastFallback(message, type);
    }
};

window.showSuccessToast = (message, duration = 3000) => {
    try {
        return getToastInstance().success(message, duration);
    } catch (e) {
        console.error('showSuccessToast error:', e);
        // 降级到简单toast
        showSimpleToastFallback(message, 'success');
    }
};

window.showErrorToast = (message, duration = 3000) => {
    try {
        return getToastInstance().error(message, duration);
    } catch (e) {
        console.error('showErrorToast error:', e);
        // 降级到简单toast
        showSimpleToastFallback(message, 'error');
    }
};

window.showWarningToast = (message, duration = 3000) => {
    try {
        return getToastInstance().warning(message, duration);
    } catch (e) {
        console.error('showWarningToast error:', e);
        // 降级到简单toast
        showSimpleToastFallback(message, 'warning');
    }
};

window.showInfoToast = (message, duration = 3000) => {
    try {
        return getToastInstance().info(message, duration);
    } catch (e) {
        console.error('showInfoToast error:', e);
        // 降级到简单toast
        showSimpleToastFallback(message, 'info');
    }
};

// 降级简单toast函数
function showSimpleToastFallback(message, type = 'success') {
    try {
        // 移除现有的简单toast
        const existingToasts = document.querySelectorAll('.simple-toast');
        existingToasts.forEach(toast => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        });
        
        // 创建新的toast
        const toast = document.createElement('div');
        toast.className = `simple-toast simple-toast-${type}`;
        toast.textContent = message;
        
        // 添加到body
        document.body.appendChild(toast);
        
        // 3秒后自动移除
        setTimeout(() => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, 3000);
        
        return toast;
    } catch (e) {
        console.error('showSimpleToastFallback error:', e);
        // 最终降级：使用alert
        alert(message);
    }
}

// 兼容性：保留全局Toast实例
window.Toast = {
    show: window.showToast,
    success: window.showSuccessToast,
    error: window.showErrorToast,
    warning: window.showWarningToast,
    info: window.showInfoToast,
    init: () => {
        const instance = getToastInstance();
        if (instance && typeof instance.init === 'function') {
            instance.init();
        }
    }
};
