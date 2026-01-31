let clipboardHistory = [];
let selectedIndex = -1;
let isVisible = false;

let invoke, listen;

async function initializeApp() {
    if (document.readyState === 'loading') {
        await new Promise(resolve => document.addEventListener('DOMContentLoaded', resolve));
    }
    invoke = window.__TAURI__.core.invoke;
    listen = window.__TAURI__.event.listen;
    await init();
}


async function init() {
    try {
        await listen('show-window', (event) => {
            showWindow(event.payload);
        });

        window.addEventListener('blur', async () => {
            try {
                await invoke('window_blur');
                hideWindow();
            } catch (error) {
                console.error('调用 window_blur 失败:', error);
            }
        });

    } catch (error) {
        console.error('初始化失败:', error);
    }
}

window.addEventListener("DOMContentLoaded", initializeApp);

document.addEventListener('keydown', (event) => {
    if (!isVisible) return;
    switch (event.key) {
        case 'ArrowLeft':
            event.preventDefault();
            if (clipboardHistory.length > 0) {
                const newIndex = selectedIndex > 0 ? selectedIndex - 1 : 0;
                updateSelection(newIndex, true); // 键盘导航时自动滚动
            }
            break;
        case 'ArrowRight':
            event.preventDefault();
            if (clipboardHistory.length > 0) {
                const newIndex = selectedIndex < clipboardHistory.length - 1 ? selectedIndex + 1 : clipboardHistory.length - 1;
                updateSelection(newIndex, true); // 键盘导航时自动滚动
            }
            break;
        case 'Enter':
            event.preventDefault();
            if (selectedIndex >= 0 && selectedIndex < clipboardHistory.length) {
                selectAndFillDirect(selectedIndex).then(r => {
                    console.log('selectAndFillDirect', r);
                });
            }
            break;
    }
});

function handleClick(index) {
    updateSelection(index);
}

function handleDoubleClick(index) {
    selectAndFillDirect(index).then(r => {
        console.log('selectAndFillDirect', r);
    });
}

async function showWindow(data) {
    let history, selectedIndex;
    history = Array.isArray(data.history) ? data.history : [];
    selectedIndex = data.selectedIndex !== undefined ? data.selectedIndex : 0;

    clipboardHistory = history;

    render();

    if (clipboardHistory.length > 0 && selectedIndex >= 0 && selectedIndex < clipboardHistory.length) {
        updateSelection(selectedIndex, true);
    } else if (clipboardHistory.length > 0) {
        updateSelection(0, true);
    }
    isVisible = true;
}

function hideWindow() {
    clipboardHistory = [];
    isVisible = false;
}

// 渲染列表
function render() {
    const content = document.getElementById('content');
    if (!content) {
        console.warn('找不到 content 元素');
        return;
    }
    content.innerHTML = '';
    if (clipboardHistory.length === 0) {
        content.innerHTML = `
                    <div class="empty-state">
                        <div class="icon">📭</div>
                        <div class="text">暂无剪切板记录</div>
                        <div class="hint">复制内容后会自动添加</div>
                    </div>
                `;
        return;
    }

    content.innerHTML = clipboardHistory.map((item, index) => `
                <div class="clipboard-item ${index === selectedIndex ? 'selected' : ''}"
                     data-index="${index}">
                    <div class="delete-btn" data-index="${index}">X</div>
                    <div class="index">${index + 1}</div>
                    <div class="content">${escapeHtml(item)}</div>
                </div>
            `).join('');

    document.querySelectorAll('.clipboard-item').forEach((item, index) => {
        item.addEventListener('click', () => handleClick(index));
        item.addEventListener('dblclick', () => handleDoubleClick(index));
    });

    document.querySelectorAll('.delete-btn').forEach((btn, index) => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            deleteItem(index).then(r => {
                console.log('deleteItem', r);
            });
        });
    });

    addDragScrolling(content);
}

function addDragScrolling(element) {
    let isDown = false;
    let startX;
    let scrollLeft;

    element.addEventListener('mousedown', (e) => {
        isDown = true;
        startX = e.pageX - element.offsetLeft;
        scrollLeft = element.scrollLeft;
        element.style.cursor = 'grabbing';
    });

    element.addEventListener('mouseleave', () => {
        isDown = false;
        element.style.cursor = 'default';
    });

    element.addEventListener('mouseup', () => {
        isDown = false;
        element.style.cursor = 'default';
    });

    element.addEventListener('mousemove', (e) => {
        if (!isDown) return;
        e.preventDefault();
        const x = e.pageX - element.offsetLeft;
        const walk = (x - startX) * 2; // 滚动速度
        element.scrollLeft = scrollLeft - walk;
    });
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

async function selectAndFillDirect(index) {
    try {
        await invoke('select_and_fill', {index});
        hideWindow();
    } catch (error) {
        console.error('填充内容失败:', error);
    }
}

function updateSelection(index, shouldScroll = false) {
    if (index < 0 || index >= clipboardHistory.length) return;
    if (selectedIndex === index) return;
    selectedIndex = index;
    const items = document.querySelectorAll('.clipboard-item');
    items.forEach(item => {
        item.classList.remove('selected');
    });
    if (items[index]) {
        items[index].classList.add('selected');
    }
    if (shouldScroll && items[index]) {
        items[index].scrollIntoView({
            behavior: 'smooth',
            block: 'nearest',
            inline: 'center'
        });
    }
}

async function deleteItem(index) {
    console.log('deleteItem', index);
    try {
        const items = document.querySelectorAll('.clipboard-item');
        if (index >= items.length) {
            console.error('索引超出范围');
            return;
        }
        const deletedItem = items[index];
        deletedItem.classList.add('deleting');
        for (let i = index + 1; i < items.length; i++) {
            items[i].classList.add('moving-left');
        }
        await new Promise(resolve => setTimeout(resolve, 300));
        invoke('remove_clipboard_item', {index}).then(() => {
            if (clipboardHistory.length > 0) {
                if (selectedIndex >= clipboardHistory.length) {
                    selectedIndex = clipboardHistory.length - 1;
                }
                updateSelection(selectedIndex);
                invoke('get_clipboard_history').then(r => {
                    clipboardHistory = r;
                    render();
                });
            } else {
                selectedIndex = -1;
            }
        });
    } catch (error) {
        console.error('删除项目失败:', error);
    }
}
