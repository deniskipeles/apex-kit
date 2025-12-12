// =========================== /teamspace/studios/this_studio/tinybase/tinybase/admin-ui/src/components/GeminiEditor.tsx ===========================
import React, { useState, useRef, useEffect } from 'react';
import { enhanceTextWithGemini, generateImageWithGemini, editImageWithGemini, GeminiResponse } from '../services/geminiService';
import {
    BoldIcon, ItalicIcon, MagicWandIcon, CloseIcon, GoogleIcon, LinkIcon, BulletedListIcon,
    NumberedListIcon, AlignLeftIcon, AlignCenterIcon, AlignRightIcon, AlignJustifyIcon, UnderlineIcon,
    StrikethroughIcon, ChevronDownIcon, ChevronUpIcon, ChevronRightIcon, ImageIcon, TableIcon, CodeBlockIcon,
    SourceIcon, FullscreenIcon, ExitFullscreenIcon, TextColorIcon, UndoIcon, RedoIcon, CopyIcon, CheckIcon, MarkdownIcon,
    TrashIcon, PlusIcon, UploadIcon
} from './icons';
import { GroundingChunk } from '../types';

// --- REPLACED CDN GLOBALS WITH IMPORTS ---
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import TurndownService from 'turndown';
import { gfm } from 'turndown-plugin-gfm';

// Initialize Turndown
const turndownService = new TurndownService({
    headingStyle: 'atx',
    codeBlockStyle: 'fenced',
    emDelimiter: '*'
});
turndownService.use(gfm);

// Configure marked renderer
const renderer = new marked.Renderer();

renderer.heading = function ({ tokens, depth }: any) {
    const text = (this as any).parser.parseInline(tokens);
    const classes = [
        'text-4xl font-bold mb-4',
        'text-3xl font-bold mb-3',
        'text-2xl font-bold mb-2',
        'text-xl font-bold mb-2',
        'text-lg font-bold mb-1',
        'text-base font-bold mb-1'
    ][depth - 1] || 'font-bold';
    return `<h${depth} class="${classes}">${text}</h${depth}>\n`;
};

renderer.paragraph = function ({ tokens }: any) {
    const text = (this as any).parser.parseInline(tokens);
    return `<p class="mb-4">${text}</p>\n`;
};

// FIX: Manually iterate over list items instead of passing 'items' array to parser.parse
renderer.list = function (token: any) {
    const ordered = token.ordered;
    const start = token.start;

    let body = '';
    for (let i = 0; i < token.items.length; i++) {
        body += this.listitem(token.items[i]);
    }

    const type = ordered ? 'list-decimal' : 'list-disc';
    const startAttr = ordered && start !== 1 ? ` start="${start}"` : '';
    return `<ul class="${type} ml-6 mb-4"${startAttr}>${body}</ul>\n`;
};

renderer.listitem = function (token: any) {
    const text = (this as any).parser.parse(token.tokens);
    return `<li class="mb-1">${text}</li>\n`;
};

renderer.link = function ({ href, title, tokens }: any) {
    const text = (this as any).parser.parseInline(tokens);
    return `<a href="${href}" class="text-blue-600 underline hover:text-blue-800" target="_blank">${text}</a>`;
};

renderer.blockquote = function ({ tokens }: any) {
    const text = (this as any).parser.parse(tokens);
    return `<blockquote class="border-l-4 border-gray-300 pl-4 italic mb-4">${text}</blockquote>\n`;
};

marked.use({
    renderer,
    breaks: true,
    gfm: true
});

const AUTO_SAVE_INTERVAL = 30000; // 30 seconds
const STORAGE_KEY = 'gemini_editor_draft';

interface GeminiEditorProps {
    value: string;
    onChange: (value: string) => void;
}

interface ImageInsertionModalProps {
    onClose: () => void;
    onInsert: (html: string) => void;
}


const ImageInsertionModal: React.FC<ImageInsertionModalProps> = ({ onClose, onInsert }) => {

    const [images, setImages] = useState<string[]>([]);
    const [currentInput, setCurrentInput] = useState('');
    const [cols, setCols] = useState(2);
    const [gap, setGap] = useState<'gap-2' | 'gap-4' | 'gap-8'>('gap-4');
    const [isRounded, setIsRounded] = useState(true);
    const [hasShadow, setHasShadow] = useState(true);
    const [aspectRatio, setAspectRatio] = useState<'aspect-auto' | 'aspect-square' | 'aspect-video'>('aspect-auto');
    const [aiPrompt, setAiPrompt] = useState('');
    const [isGenerating, setIsGenerating] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);
    const [editingIndex, setEditingIndex] = useState<number | null>(null);
    const [editPrompt, setEditPrompt] = useState('');
    const [isEditing, setIsEditing] = useState(false);

    const handleAddImage = () => {
        if (currentInput.trim()) {
            setImages([...images, currentInput.trim()]);
            setCurrentInput('');
        }
    };

    const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
        if (e.target.files && e.target.files[0]) {
            const file = e.target.files[0];
            const reader = new FileReader();
            reader.onloadend = () => {
                if (reader.result) {
                    setImages([...images, reader.result as string]);
                }
            };
            reader.readAsDataURL(file);
        }
    };

    const handleGenerateImage = async () => {
        if (!aiPrompt.trim()) return;
        setIsGenerating(true);
        try {
            const base64 = await generateImageWithGemini(aiPrompt);
            setImages([...images, base64]);
            setAiPrompt('');
        } catch (e) {
            console.error(e);
            alert("Failed to generate image. Please try again.");
        } finally {
            setIsGenerating(false);
        }
    };

    const handleEditImage = async () => {
        if (editingIndex === null || !editPrompt.trim()) return;
        setIsEditing(true);
        try {
            const base64 = await editImageWithGemini(images[editingIndex], editPrompt);
            const newImages = [...images];
            newImages[editingIndex] = base64;
            setImages(newImages);
            setEditingIndex(null);
            setEditPrompt('');
        } catch (e) {
            console.error(e);
            alert("Failed to edit image. Please try again.");
        } finally {
            setIsEditing(false);
        }
    };

    const removeImage = (index: number) => {
        const newImages = [...images];
        newImages.splice(index, 1);
        setImages(newImages);
        if (editingIndex === index) setEditingIndex(null);
    };

    const generateHtml = () => {
        const gridClass = `grid grid-cols-1 sm:grid-cols-${Math.min(cols, 2)} md:grid-cols-${cols} ${gap} my-4`;
        const imgClass = `w-full h-full object-cover ${isRounded ? 'rounded-lg' : ''} ${hasShadow ? 'shadow-md' : ''} ${aspectRatio}`;

        let html = `<div class="${gridClass}">`;
        images.forEach(img => {
            html += `<div class="overflow-hidden ${isRounded ? 'rounded-lg' : ''}"><img src="${img}" class="${imgClass}" alt="Inserted image" /></div>`;
        });
        html += `</div><p><br></p>`;
        return html;
    };

    const handleInsert = () => {
        if (images.length > 0) {
            onInsert(generateHtml());
            onClose();
        }
    };

    return (
        <div className="fixed inset-0 bg-gray-900 bg-opacity-75 flex items-center justify-center z-50 p-4 transition-opacity duration-300">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-6xl h-[90vh] flex flex-col animate-fade-in-up overflow-hidden">
                <div className="flex justify-between items-center p-4 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 z-10">
                    <h2 className="text-xl font-bold text-gray-900 dark:text-white">
                        {editingIndex !== null ? 'AI Image Editor' : 'Insert Images & Layout'}
                    </h2>
                    <button onClick={onClose} className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
                        <CloseIcon className="w-6 h-6" />
                    </button>
                </div>
                <div className="flex flex-col md:flex-row flex-1 overflow-hidden">
                    <div className="w-full md:w-[24rem] md:shrink-0 flex flex-col border-b md:border-b-0 md:border-r border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 overflow-y-auto max-h-[40vh] md:max-h-full order-2 md:order-1">
                        <div className="p-4 space-y-6">
                            {editingIndex !== null ? (
                                <div className="animate-fade-in-up">
                                    <div className="mb-4">
                                        <button onClick={() => setEditingIndex(null)} className="text-sm text-blue-600 dark:text-blue-400 hover:underline mb-2 flex items-center gap-1">
                                            <ChevronRightIcon className="w-4 h-4 rotate-180" /> Back to Layout
                                        </button>
                                        <div className="rounded-lg overflow-hidden border border-gray-300 dark:border-gray-600 mb-4 bg-gray-200 dark:bg-gray-800 flex items-center justify-center min-h-[150px]">
                                            <img src={images[editingIndex]} alt="Editing target" className="max-w-full max-h-[200px] object-contain" />
                                        </div>
                                    </div>
                                    <div>
                                        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Instructions</label>
                                        <textarea value={editPrompt} onChange={(e) => setEditPrompt(e.target.value)} placeholder="e.g., Make it a painting in Van Gogh style..." className="w-full p-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm dark:bg-gray-800 dark:text-white mb-2 focus:ring-2 focus:ring-blue-500" rows={3} />
                                        <div className="flex gap-2">
                                            <button onClick={handleEditImage} disabled={isEditing || !editPrompt.trim()} className="flex-1 py-2 px-4 bg-purple-600 hover:bg-purple-700 text-white rounded-md text-sm disabled:opacity-50 flex items-center justify-center gap-2 transition-colors">{isEditing ? 'Processing...' : <><MagicWandIcon className="w-4 h-4" /> Generate Edit</>}</button>
                                            <button onClick={() => setEditingIndex(null)} className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-700 dark:text-gray-300">Cancel</button>
                                        </div>
                                    </div>
                                </div>
                            ) : (
                                <>
                                    <div>
                                        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">Add Image</label>
                                        <div className="flex gap-2 mb-2">
                                            <input type="text" value={currentInput} onChange={(e) => setCurrentInput(e.target.value)} placeholder="https://..." className="flex-1 p-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm dark:bg-gray-800 dark:text-white focus:ring-2 focus:ring-blue-500" onKeyDown={(e) => e.key === 'Enter' && handleAddImage()} />
                                            <button onClick={handleAddImage} className="p-2 bg-blue-600 text-white rounded-md hover:bg-blue-700"><PlusIcon className="w-5 h-5" /></button>
                                        </div>
                                        <div className="grid grid-cols-2 gap-2 mb-2">
                                            <button onClick={() => fileInputRef.current?.click()} className="py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-md text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-800 flex items-center justify-center gap-1"><UploadIcon className="w-3 h-3" /> Upload</button>
                                            <input type="file" ref={fileInputRef} className="hidden" accept="image/*" onChange={handleFileUpload} />
                                        </div>
                                        <div className="p-3 bg-purple-50 dark:bg-gray-800 border border-purple-100 dark:border-purple-900 rounded-md">
                                            <label className="block text-xs font-bold text-purple-700 dark:text-purple-400 mb-1">AI Image Gen</label>
                                            <div className="flex gap-2">
                                                <input value={aiPrompt} onChange={(e) => setAiPrompt(e.target.value)} placeholder="Describe..." className="flex-1 p-1.5 border border-purple-200 dark:border-gray-600 rounded text-xs dark:bg-gray-900 dark:text-white focus:ring-1 focus:ring-purple-500" onKeyDown={(e) => e.key === 'Enter' && !e.shiftKey && handleGenerateImage()} />
                                                <button onClick={handleGenerateImage} disabled={isGenerating || !aiPrompt.trim()} className="p-1.5 bg-purple-600 hover:bg-purple-700 text-white rounded text-xs disabled:opacity-50">{isGenerating ? '...' : <MagicWandIcon className="w-4 h-4" />}</button>
                                            </div>
                                        </div>
                                    </div>
                                    <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
                                        <div className="mb-4">
                                            <div className="flex justify-between items-center mb-1"><label className="text-xs font-medium text-gray-700 dark:text-gray-300">Columns: {cols}</label></div>
                                            <input type="range" min="1" max="4" value={cols} onChange={(e) => setCols(parseInt(e.target.value))} className="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer dark:bg-gray-700" />
                                        </div>
                                        <div className="grid grid-cols-2 gap-4 mb-4">
                                            <div><label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">Gap</label><select value={gap} onChange={(e) => setGap(e.target.value as any)} className="w-full p-1.5 border border-gray-300 dark:border-gray-600 rounded text-xs dark:bg-gray-800 dark:text-white"><option value="gap-2">Small</option><option value="gap-4">Medium</option><option value="gap-8">Large</option></select></div>
                                            <div><label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">Ratio</label><select value={aspectRatio} onChange={(e) => setAspectRatio(e.target.value as any)} className="w-full p-1.5 border border-gray-300 dark:border-gray-600 rounded text-xs dark:bg-gray-800 dark:text-white"><option value="aspect-auto">Natural</option><option value="aspect-square">Square</option><option value="aspect-video">Video</option></select></div>
                                        </div>
                                        <div className="flex gap-4">
                                            <label className="flex items-center gap-2 cursor-pointer"><input type="checkbox" checked={isRounded} onChange={(e) => setIsRounded(e.target.checked)} className="rounded text-blue-600 w-3 h-3" /><span className="text-xs text-gray-700 dark:text-gray-300">Rounded</span></label>
                                            <label className="flex items-center gap-2 cursor-pointer"><input type="checkbox" checked={hasShadow} onChange={(e) => setHasShadow(e.target.checked)} className="rounded text-blue-600 w-3 h-3" /><span className="text-xs text-gray-700 dark:text-gray-300">Shadow</span></label>
                                        </div>
                                    </div>
                                    <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
                                        <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-2">Images ({images.length})</h3>
                                        <ul className="space-y-2">
                                            {images.map((img, idx) => (
                                                <li key={idx} className="flex items-center justify-between p-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
                                                    <div className="flex items-center gap-2 overflow-hidden"><img src={img} alt="" className="w-8 h-8 object-cover rounded" /><span className="text-xs text-gray-500 truncate max-w-[80px] sm:max-w-[100px]">{img.substring(0, 15)}...</span></div>
                                                    <div className="flex items-center gap-1"><button onClick={() => setEditingIndex(idx)} className="p-1 text-purple-500 hover:text-purple-700 bg-purple-50 dark:bg-gray-700 hover:bg-purple-100 rounded"><MagicWandIcon className="w-3.5 h-3.5" /></button><button onClick={() => removeImage(idx)} className="p-1 text-red-500 hover:text-red-700 bg-red-50 dark:bg-gray-700 hover:bg-red-100 rounded"><TrashIcon className="w-3.5 h-3.5" /></button></div>
                                                </li>
                                            ))}
                                        </ul>
                                    </div>
                                </>
                            )}
                        </div>
                    </div>
                    <div className="flex-1 bg-gray-100 dark:bg-gray-950 p-4 md:p-6 overflow-y-auto order-1 md:order-2 min-h-[30vh]">
                        <div className="bg-white dark:bg-gray-800 rounded shadow-sm border border-gray-200 dark:border-gray-700 p-4 md:p-8 min-h-full">
                            <div className={`grid grid-cols-1 sm:grid-cols-${Math.min(cols, 2)} md:grid-cols-${cols} ${gap}`}>
                                {images.map((img, idx) => (
                                    <div key={idx} className={`relative group overflow-hidden ${isRounded ? 'rounded-lg' : ''} ${editingIndex === idx ? 'ring-2 ring-purple-500 ring-offset-2' : ''}`}>
                                        <img src={img} alt={`Preview ${idx}`} className={`w-full h-full object-cover ${isRounded ? 'rounded-lg' : ''} ${hasShadow ? 'shadow-md' : ''} ${aspectRatio}`} />
                                    </div>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>
                <div className="p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 flex justify-end gap-3 z-10">
                    <button onClick={onClose} className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 transition">Cancel</button>
                    <button onClick={handleInsert} disabled={images.length === 0} className="px-6 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed transition font-medium">Insert Layout</button>
                </div>
            </div>
        </div>
    );
};

// ... (Include AiModal, CitationItem, ToolbarButton, GeminiEditor implementation)
interface AiModalProps {
    onClose: () => void;
    aiPrompt: string;
    onAiPromptChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => void;
    handleAiRequest: () => void;
    isLoading: boolean;
    error: string | null;
    selectedText: string;
    suggestions: string[];
    onSuggestionSelect: (suggestion: string) => void;
    onQuickAction: (action: 'spellcheck' | 'inflate') => void;
}
const AiModal: React.FC<AiModalProps> = ({ onClose, aiPrompt, onAiPromptChange, handleAiRequest, isLoading, error, selectedText, suggestions, onSuggestionSelect, onQuickAction }) => (
    <div className="fixed inset-0 bg-gray-900 bg-opacity-75 flex items-center justify-center z-50 p-4 transition-opacity duration-300">
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-lg p-6 relative animate-fade-in-up">
            <button onClick={onClose} className="absolute top-4 right-4 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"><CloseIcon className="w-6 h-6" /></button>
            <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-4">Magic Wand</h2>
            {selectedText && (<div className="mb-4 p-3 bg-gray-100 dark:bg-gray-700 rounded-md border border-gray-200 dark:border-gray-600"><p className="text-sm text-gray-600 dark:text-gray-400 font-semibold">Selected Text:</p><p className="text-sm text-gray-800 dark:text-gray-200 italic line-clamp-3">"{selectedText}"</p></div>)}
            {selectedText && (<div className="mb-4 pt-4 border-t border-gray-200 dark:border-gray-600"><p className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2">Quick Actions:</p><div className="flex gap-2"><button onClick={() => onQuickAction('spellcheck')} disabled={isLoading} className="flex-1 px-4 py-2 text-sm text-center bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-200 rounded-md hover:bg-gray-300 dark:hover:bg-gray-600 transition disabled:opacity-50 disabled:cursor-not-allowed">Correct</button><button onClick={() => onQuickAction('inflate')} disabled={isLoading} className="flex-1 px-4 py-2 text-sm text-center bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-200 rounded-md hover:bg-gray-300 dark:hover:bg-gray-600 transition disabled:opacity-50 disabled:cursor-not-allowed">Expand</button></div></div>)}
            <p className="text-gray-600 dark:text-gray-400 mb-2">{selectedText ? 'Or describe a custom action:' : 'Describe what you want to create or modify.'}</p>
            <div className="flex flex-wrap gap-2 mb-4">{suggestions.map((suggestion, index) => (<button key={index} onClick={() => onSuggestionSelect(suggestion)} className="px-3 py-1 text-sm bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-200 rounded-full hover:bg-gray-300 dark:hover:bg-gray-600 transition">{suggestion}</button>))}</div>
            <textarea value={aiPrompt} onChange={onAiPromptChange} placeholder="e.g., Turn this into a poem" className="w-full p-3 border border-gray-300 dark:border-gray-600 rounded-md bg-gray-50 dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-blue-500 transition" rows={3} autoFocus />
            {error && <p className="text-red-500 text-sm mt-2">{error}</p>}
            <div className="mt-6 flex justify-end space-x-3"><button onClick={onClose} className="px-4 py-2 rounded-md text-gray-700 bg-gray-200 hover:bg-gray-300 dark:bg-gray-600 dark:text-gray-200 dark:hover:bg-gray-500 transition">Cancel</button><button onClick={handleAiRequest} disabled={isLoading || !aiPrompt.trim()} className="px-4 py-2 rounded-md text-white bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 disabled:cursor-not-allowed flex items-center transition">{isLoading ? 'Processing...' : 'Generate'}</button></div>
        </div>
    </div>
);

const CitationItem: React.FC<{ citation: GroundingChunk, index: number }> = ({ citation, index }) => {
    const [copied, setCopied] = useState(false);
    const handleCopy = () => { navigator.clipboard.writeText(citation.web.uri); setCopied(true); setTimeout(() => setCopied(false), 2000); };
    return (
        <li className="flex items-center justify-between p-3 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg hover:shadow-md transition-shadow group">
            <div className="flex items-center overflow-hidden mr-3"><span className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full bg-blue-100 dark:bg-blue-900 text-blue-600 dark:text-blue-300 text-xs font-bold mr-3">{index + 1}</span><div className="flex flex-col min-w-0"><a href={citation.web.uri} target="_blank" rel="noopener noreferrer" className="text-sm font-medium text-gray-900 dark:text-gray-100 hover:text-blue-600 dark:hover:text-blue-400 truncate transition-colors">{citation.web.title || "Untitled Source"}</a><a href={citation.web.uri} target="_blank" rel="noopener noreferrer" className="text-xs text-gray-500 dark:text-gray-400 truncate hover:underline">{citation.web.uri}</a></div></div>
            <button onClick={handleCopy} className="flex-shrink-0 p-2 text-gray-400 hover:text-gray-600 dark:text-gray-500 dark:hover:text-gray-300 bg-gray-50 dark:bg-gray-700 rounded-md hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors" title="Copy Link">{copied ? <CheckIcon className="w-4 h-4 text-green-500" /> : <CopyIcon className="w-4 h-4" />}</button>
        </li>
    );
};

const suggestionsWithSelection = ['Summarize this', 'Improve writing', 'Make it more professional', 'Change the tone to be more casual'];
const suggestionsWithoutSelection = ['Write a blog post about...', 'Brainstorm ideas for a marketing campaign', 'Draft a professional email to a client', 'Create a bulleted list of pros and cons for...'];
const ToolbarButton: React.FC<{ onClick: (e: React.MouseEvent<HTMLButtonElement>) => void; children: React.ReactNode; tooltip: string, buttonRef?: React.Ref<HTMLButtonElement>, disabled?: boolean }> = ({ onClick, children, tooltip, buttonRef, disabled }) => (<button ref={buttonRef} onClick={onClick} onMouseDown={(e) => e.preventDefault()} disabled={disabled} className={`p-2 rounded relative group flex-shrink-0 transition-colors ${disabled ? 'text-gray-300 dark:text-gray-600 cursor-not-allowed' : 'text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700'}`} aria-label={tooltip}>{children}<span className="absolute bottom-full mb-2 w-max px-2 py-1 text-xs text-white bg-gray-900 rounded opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none z-50">{tooltip}</span></button>);
const fontFamilies = [{ name: 'Arial', value: 'Arial, Helvetica, sans-serif' }, { name: 'Georgia', value: 'Georgia, serif' }, { name: 'Times New Roman', value: "'Times New Roman', Times, serif" }, { name: 'Courier New', value: "'Courier New', Courier, monospace" }, { name: 'Verdana', value: 'Verdana, Geneva, sans-serif' }, { name: 'Comic Sans MS', value: "'Comic Sans MS', cursive, sans-serif" }];
const fontSizes = [10, 12, 14, 18, 24, 36];
const colors = ['#000000', '#4D4D4D', '#999999', '#FFFFFF', '#E6194B', '#F58231', '#FFE119', '#3CB44B', '#4363D8', '#911EB4', '#42D4F4', '#FABED4', '#FFD8B1', '#FFFAC8', '#AAFFC3', '#469990'];

const GeminiEditor: React.FC<GeminiEditorProps> = ({ value, onChange }) => {
    const editorRef = useRef<HTMLDivElement>(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [aiPrompt, setAiPrompt] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [citations, setCitations] = useState<GroundingChunk[]>([]);
    const [isCitationsOpen, setIsCitationsOpen] = useState(false);
    const [selectedText, setSelectedText] = useState('');
    const [savedRange, setSavedRange] = useState<Range | null>(null);
    const [activeDropdown, setActiveDropdown] = useState<string | null>(null);
    const [dropdownPosition, setDropdownPosition] = useState<{ top: number, left: number } | null>(null);
    const [activeSubMenu, setActiveSubMenu] = useState<string | null>(null);
    const [tableGridSize, setTableGridSize] = useState({ rows: 1, cols: 1 });
    const [isSourceMode, setIsSourceMode] = useState(false);
    const [isMarkdownMode, setIsMarkdownMode] = useState(false);
    const [isFullScreen, setIsFullScreen] = useState(false);
    const [sourceContent, setSourceContent] = useState('');
    const [markdownContent, setMarkdownContent] = useState('');
    const [isImageModalOpen, setIsImageModalOpen] = useState(false);
    const [showRestorePrompt, setShowRestorePrompt] = useState(false);
    const [draftContent, setDraftContent] = useState<string | null>(null);
    const valueRef = useRef(value);
    const [history, setHistory] = useState<string[]>([value]);
    const [historyIndex, setHistoryIndex] = useState(0);
    const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const paragraphBtnRef = useRef<HTMLButtonElement>(null);
    const fontBtnRef = useRef<HTMLButtonElement>(null);
    const fontSizeBtnRef = useRef<HTMLButtonElement>(null);
    const colorBtnRef = useRef<HTMLButtonElement>(null);
    const tableBtnRef = useRef<HTMLButtonElement>(null);
    const dropdownPanelRef = useRef<HTMLDivElement>(null);

    useEffect(() => { valueRef.current = value; }, [value]);
    useEffect(() => { const saved = localStorage.getItem(STORAGE_KEY); if (saved && saved !== valueRef.current) { setDraftContent(saved); setShowRestorePrompt(true); } const intervalId = setInterval(() => { if (valueRef.current) { localStorage.setItem(STORAGE_KEY, valueRef.current); } }, AUTO_SAVE_INTERVAL); return () => clearInterval(intervalId); }, []);
    useEffect(() => { if (editorRef.current && editorRef.current.innerHTML !== value && !isSourceMode && !isMarkdownMode) { editorRef.current.innerHTML = value; } }, [value, isSourceMode, isMarkdownMode]);
    useEffect(() => { if (isSourceMode) { setSourceContent(value); } }, [value, isSourceMode]);
    useEffect(() => { const handleClickOutside = (event: MouseEvent) => { if (dropdownPanelRef.current && !dropdownPanelRef.current.contains(event.target as Node)) { const isToolbarButtonClick = paragraphBtnRef.current?.contains(event.target as Node) || fontBtnRef.current?.contains(event.target as Node) || fontSizeBtnRef.current?.contains(event.target as Node) || colorBtnRef.current?.contains(event.target as Node) || tableBtnRef.current?.contains(event.target as Node); if (!isToolbarButtonClick) { setActiveDropdown(null); } } }; document.addEventListener('mousedown', handleClickOutside); return () => { document.removeEventListener('mousedown', handleClickOutside); }; }, []);

    const addToHistory = (content: string) => { if (history[historyIndex] === content) return; const newHistory = history.slice(0, historyIndex + 1); newHistory.push(content); setHistory(newHistory); setHistoryIndex(newHistory.length - 1); };
    const handleRestoreDraft = () => { if (draftContent) { onChange(draftContent); addToHistory(draftContent); if (editorRef.current) editorRef.current.innerHTML = draftContent; setShowRestorePrompt(false); setDraftContent(null); } };
    const handleDiscardDraft = () => { localStorage.removeItem(STORAGE_KEY); setShowRestorePrompt(false); setDraftContent(null); };
    const handleUndo = () => { if (historyIndex > 0) { const newIndex = historyIndex - 1; const content = history[newIndex]; setHistoryIndex(newIndex); onChange(content); if (editorRef.current && !isSourceMode && !isMarkdownMode) editorRef.current.innerHTML = content; if (isSourceMode) setSourceContent(content); if (isMarkdownMode) setMarkdownContent(turndownService.turndown(content)); } };
    const handleRedo = () => { if (historyIndex < history.length - 1) { const newIndex = historyIndex + 1; const content = history[newIndex]; setHistoryIndex(newIndex); onChange(content); if (editorRef.current && !isSourceMode && !isMarkdownMode) editorRef.current.innerHTML = content; if (isSourceMode) setSourceContent(content); if (isMarkdownMode) setMarkdownContent(turndownService.turndown(content)); } };
    const handleKeyDown = (e: React.KeyboardEvent) => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') { e.preventDefault(); if (e.shiftKey) { handleRedo(); } else { handleUndo(); } } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'y') { e.preventDefault(); handleRedo(); } };
    const handleDropdownToggle = (type: 'paragraph' | 'font' | 'fontSize' | 'color' | 'table', ref: React.RefObject<HTMLButtonElement>) => { if (isMarkdownMode) return; if (activeDropdown === type) { setActiveDropdown(null); return; } const rect = ref.current?.getBoundingClientRect(); if (rect) { setDropdownPosition({ top: rect.bottom + 4, left: rect.left }); setActiveDropdown(type); setActiveSubMenu(null); if (type === 'table') { setTableGridSize({ rows: 1, cols: 1 }); } } };
    const handleInput = (e: React.FormEvent<HTMLDivElement>) => { const content = e.currentTarget.innerHTML; onChange(content); if (timeoutRef.current) clearTimeout(timeoutRef.current); timeoutRef.current = setTimeout(() => { addToHistory(content); }, 1000); };
    const handleSourceChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => { setSourceContent(e.target.value); onChange(e.target.value); if (timeoutRef.current) clearTimeout(timeoutRef.current); timeoutRef.current = setTimeout(() => { addToHistory(e.target.value); }, 1000); };
    const handleMarkdownChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => { const newMarkdown = e.target.value; setMarkdownContent(newMarkdown); if (timeoutRef.current) clearTimeout(timeoutRef.current); timeoutRef.current = setTimeout(() => { const html = marked.parse(newMarkdown) as string; const clean = DOMPurify.sanitize(html); onChange(clean); addToHistory(clean); }, 1000); };

    const applyFormat = (command: string, valueArg: string | null = null) => { document.execCommand(command, false, valueArg || undefined); if (editorRef.current) { editorRef.current.focus(); const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } setActiveDropdown(null); };
    const applyFontSize = (size: number) => { const selection = window.getSelection(); if (!selection || !selection.rangeCount) { setActiveDropdown(null); return; } const range = selection.getRangeAt(0); if (range.collapsed) { setActiveDropdown(null); return; } const applyStyleToNode = (node: Node): Node => { if (node.nodeType === Node.TEXT_NODE && node.nodeValue?.trim()) { const span = document.createElement('span'); span.style.fontSize = `${size}px`; span.textContent = node.nodeValue; return span; } else if (node.nodeType === Node.ELEMENT_NODE) { const element = node as HTMLElement; const newElement = element.cloneNode(false) as HTMLElement; const isBlock = window.getComputedStyle(element).display === 'block' || ['LI', 'TABLE', 'TR', 'TH', 'TD'].includes(element.tagName); if (isBlock) { element.childNodes.forEach(child => newElement.appendChild(applyStyleToNode(child))); } else { newElement.style.fontSize = `${size}px`; element.childNodes.forEach(child => newElement.appendChild(child.cloneNode(true))); } return newElement; } return node.cloneNode(true); }; try { const fragment = range.extractContents(); const styledFragment = document.createDocumentFragment(); Array.from(fragment.childNodes).forEach(child => { styledFragment.appendChild(applyStyleToNode(child)); }); range.insertNode(styledFragment); range.collapse(false); selection.removeAllRanges(); selection.addRange(range); } catch (e) { console.error("Failed to apply font size:", e); } if (editorRef.current) { editorRef.current.focus(); const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } setActiveDropdown(null); };
    const applyInlineStyle = (command: string) => applyFormat(command);
    const applyInlineCode = () => { document.execCommand('insertHTML', false, `<code class="bg-gray-200 dark:bg-gray-700 px-1 py-0.5 rounded text-sm font-mono">${window.getSelection()?.toString()}</code>`); if (editorRef.current) { const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } setActiveDropdown(null); };
    const applyCodeBlock = () => { const selection = window.getSelection(); if (selection && selection.rangeCount > 0) { const range = selection.getRangeAt(0); const selectedText = range.toString(); const codeBlock = `<pre class="bg-gray-900 text-white p-4 rounded-md overflow-x-auto"><code class="font-mono">${selectedText}</code></pre><p><br></p>`; document.execCommand('insertHTML', false, codeBlock); if (editorRef.current) { const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } } };
    const applyLink = () => { const url = prompt("Enter the URL:"); if (url) { document.execCommand('createLink', false, url); if (editorRef.current) { const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } } };
    const openImageModal = () => { const selection = window.getSelection(); if (selection && selection.rangeCount > 0) { const range = selection.getRangeAt(0); setSavedRange(range.cloneRange()); } else { setSavedRange(null); } setIsImageModalOpen(true); };
    const handleImageInsert = (html: string) => { if (savedRange) { const selection = window.getSelection(); if (selection) { selection.removeAllRanges(); selection.addRange(savedRange); } } document.execCommand('insertHTML', false, html); if (editorRef.current) { const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } setSavedRange(null); };
    const insertTable = (rows: number, cols: number) => { if (rows > 0 && cols > 0) { let table = '<table class="w-full border-collapse border border-gray-400 dark:border-gray-600"><tbody>'; for (let i = 0; i < rows; i++) { table += '<tr>'; for (let j = 0; j < cols; j++) { table += '<td class="border border-gray-300 dark:border-gray-700 p-2"><p><br></p></td>'; } table += '</tr>'; } table += '</tbody></table><p><br></p>'; document.execCommand('insertHTML', false, table); if (editorRef.current) { editorRef.current.focus(); const content = editorRef.current.innerHTML; onChange(content); addToHistory(content); } } setActiveDropdown(null); };
    const toggleSourceMode = () => { if (isMarkdownMode) setIsMarkdownMode(false); if (!isSourceMode) setSourceContent(value); setIsSourceMode(!isSourceMode); };
    const toggleMarkdownMode = () => { if (isSourceMode) setIsSourceMode(false); if (!isMarkdownMode) { const markdown = turndownService.turndown(value); setMarkdownContent(markdown); setIsMarkdownMode(true); } else { const html = marked.parse(markdownContent) as string; const clean = DOMPurify.sanitize(html); onChange(clean); addToHistory(clean); setIsMarkdownMode(false); } };
    const toggleFullScreen = () => setIsFullScreen(!isFullScreen);
    const menuStructure: any = { 'Headings': [{ label: 'Heading 1', onClick: () => applyFormat('formatBlock', '<h1>') }, { label: 'Heading 2', onClick: () => applyFormat('formatBlock', '<h2>') }, { label: 'Heading 3', onClick: () => applyFormat('formatBlock', '<h3>') }, { label: 'Heading 4', onClick: () => applyFormat('formatBlock', '<h4>') }, { label: 'Heading 5', onClick: () => applyFormat('formatBlock', '<h5>') }, { label: 'Heading 6', onClick: () => applyFormat('formatBlock', '<h6>') },], 'Inline': [{ label: 'Bold', onClick: () => applyInlineStyle('bold') }, { label: 'Italic', onClick: () => applyInlineStyle('italic') }, { label: 'Underline', onClick: () => applyInlineStyle('underline') }, { label: 'Strikethrough', onClick: () => applyInlineStyle('strikeThrough') }, { label: 'Superscript', onClick: () => applyInlineStyle('superscript') }, { label: 'Subscript', onClick: () => applyInlineStyle('subscript') }, { label: 'Code', onClick: applyInlineCode },], 'Align': [{ label: 'Align Left', onClick: () => applyFormat('justifyLeft') }, { label: 'Align Center', onClick: () => applyFormat('justifyCenter') }, { label: 'Align Right', onClick: () => applyFormat('justifyRight') }, { label: 'Align Justify', onClick: () => applyFormat('justifyFull') },] };
    const mainCategories = Object.keys(menuStructure);
    const openMagicWand = () => { const selection = window.getSelection(); if (selection && selection.rangeCount > 0) { const range = selection.getRangeAt(0); setSavedRange(range.cloneRange()); setSelectedText(selection.toString().trim()); } else { setSavedRange(null); setSelectedText(''); } setAiPrompt(''); setError(null); setIsModalOpen(true); };
    const handleAiRequest = async (promptOverride?: string) => { const finalPrompt = promptOverride || aiPrompt; if (!finalPrompt.trim()) return; setIsLoading(true); setError(null); setCitations([]); setIsCitationsOpen(false); const contentToProcess = selectedText || editorRef.current?.innerText || ''; try { const result: GeminiResponse = await enhanceTextWithGemini(contentToProcess, finalPrompt); const dirtyHtml = marked.parse(result.text) as string; const cleanHtml = DOMPurify.sanitize(dirtyHtml); if (isMarkdownMode || isSourceMode) { setIsMarkdownMode(false); setIsSourceMode(false); setTimeout(() => { if (editorRef.current) { editorRef.current.innerHTML = cleanHtml; onChange(cleanHtml); addToHistory(cleanHtml); } }, 50); } else { editorRef.current?.focus(); let newContent = ''; if (selectedText && editorRef.current && savedRange) { const selection = window.getSelection(); if (selection) { selection.removeAllRanges(); selection.addRange(savedRange); } const documentFragment = savedRange.createContextualFragment(cleanHtml); const lastNode = documentFragment.lastChild; savedRange.deleteContents(); savedRange.insertNode(documentFragment); if (lastNode) { savedRange.setStartAfter(lastNode); savedRange.collapse(true); if (selection) { selection.removeAllRanges(); selection.addRange(savedRange); } } newContent = editorRef.current.innerHTML; onChange(newContent); } else { newContent = cleanHtml; onChange(newContent); } addToHistory(newContent); } if (result.metadata && result.metadata.groundingChunks) { setCitations(result.metadata.groundingChunks); setIsCitationsOpen(true); } setIsModalOpen(false); setAiPrompt(''); setSelectedText(''); setSavedRange(null); } catch (e) { setError('An error occurred. Please try again.'); console.error(e); } finally { setIsLoading(false); } };
    const handleSuggestionSelect = (suggestion: string) => { handleAiRequest(suggestion); };
    const handleQuickAction = (action: 'spellcheck' | 'inflate') => { let prompt = ''; if (action === 'spellcheck') { prompt = 'Correct spelling and grammar for the provided text. Only return the corrected text, maintaining the original meaning and tone as much as possible.'; } else if (action === 'inflate') { prompt = 'Elaborate and expand on the provided text. Add more details, examples, or explanations to make it more comprehensive. Use Google Search for up-to-date information if necessary.'; } handleAiRequest(prompt); };
    const SubMenuItem: React.FC<{ item: { label: string, onClick: () => void } }> = ({ item }) => { let content; const commonClasses = "w-full text-left block px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"; switch (item.label) { case 'Heading 1': content = <h1 className="text-3xl font-bold my-0">{item.label}</h1>; break; case 'Heading 2': content = <h2 className="text-2xl font-bold my-0">{item.label}</h2>; break; case 'Heading 3': content = <h3 className="text-xl font-bold my-0">{item.label}</h3>; break; case 'Heading 4': content = <h4 className="text-lg font-bold my-0">{item.label}</h4>; break; case 'Heading 5': content = <h5 className="text-base font-bold my-0">{item.label}</h5>; break; case 'Heading 6': content = <h6 className="text-sm font-bold my-0">{item.label}</h6>; break; case 'Bold': content = <span className="font-bold">{item.label}</span>; break; case 'Italic': content = <span className="italic">{item.label}</span>; break; case 'Underline': content = <span className="underline">{item.label}</span>; break; case 'Strikethrough': content = <span className="line-through">{item.label}</span>; break; case 'Superscript': content = <span>Text<sup>Superscript</sup></span>; break; case 'Subscript': content = <span>Text<sub>Subscript</sub></span>; break; case 'Code': content = <code className="font-mono">{item.label}</code>; break; default: content = <span>{item.label}</span>; } return <button onClick={item.onClick} className={commonClasses}>{content}</button>; };
    const editorContainerClasses = `flex flex-col bg-white dark:bg-gray-800 transition-all duration-300 ${isFullScreen ? 'fixed inset-0 z-40' : 'relative h-[70vh] w-full'} border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden`;

    return (
        <div className={editorContainerClasses}>
            <div className="flex-none border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 p-2 z-20">
                <div className="overflow-x-auto toolbar-scroll">
                    <div className="flex items-center flex-nowrap gap-x-1">

                        {/* Undo/Redo Buttons */}
                        <div className="flex items-center flex-shrink-0">
                            <ToolbarButton onClick={handleUndo} tooltip="Undo" disabled={historyIndex <= 0}>
                                <UndoIcon className="w-5 h-5" />
                            </ToolbarButton>
                            <ToolbarButton onClick={handleRedo} tooltip="Redo" disabled={historyIndex >= history.length - 1}>
                                <RedoIcon className="w-5 h-5" />
                            </ToolbarButton>
                        </div>

                        <div className="w-px h-5 bg-gray-300 dark:bg-gray-600 mx-1 flex-shrink-0"></div>

                        {/* Paragraph Dropdown Trigger */}
                        <div className="flex-shrink-0">
                            <button ref={paragraphBtnRef} onMouseDown={(e) => e.preventDefault()} disabled={isMarkdownMode} onClick={() => handleDropdownToggle('paragraph', paragraphBtnRef)} className={`flex items-center justify-between w-32 px-3 py-1.5 text-left bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-md text-sm hover:bg-gray-100 dark:hover:bg-gray-700 ${isMarkdownMode ? 'opacity-50 cursor-not-allowed' : ''}`}>
                                <span>Paragraph</span>
                                <ChevronDownIcon className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Font Family Dropdown Trigger */}
                        <div className="flex-shrink-0">
                            <button ref={fontBtnRef} onMouseDown={(e) => e.preventDefault()} disabled={isMarkdownMode} onClick={() => handleDropdownToggle('font', fontBtnRef)} className={`flex items-center justify-between w-36 px-3 py-1.5 text-left bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-md text-sm hover:bg-gray-100 dark:hover:bg-gray-700 ${isMarkdownMode ? 'opacity-50 cursor-not-allowed' : ''}`}>
                                <span>Font Family</span>
                                <ChevronDownIcon className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Font Size Dropdown Trigger */}
                        <div className="flex-shrink-0">
                            <button ref={fontSizeBtnRef} onMouseDown={(e) => e.preventDefault()} disabled={isMarkdownMode} onClick={() => handleDropdownToggle('fontSize', fontSizeBtnRef)} className={`flex items-center justify-between w-32 px-3 py-1.5 text-left bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-md text-sm hover:bg-gray-100 dark:hover:bg-gray-700 ${isMarkdownMode ? 'opacity-50 cursor-not-allowed' : ''}`}>
                                <span>Font Size</span>
                                <ChevronDownIcon className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Color Picker Dropdown Trigger */}
                        <div className="flex-shrink-0">
                            <ToolbarButton
                                buttonRef={colorBtnRef}
                                onClick={() => handleDropdownToggle('color', colorBtnRef)}
                                tooltip="Text Color"
                                disabled={isMarkdownMode}
                            >
                                <TextColorIcon className="w-5 h-5" />
                            </ToolbarButton>
                        </div>

                        <div className="w-px h-5 bg-gray-300 dark:bg-gray-600 mx-1 flex-shrink-0"></div>

                        {/* List Buttons */}
                        <div className="flex items-center flex-shrink-0">
                            <ToolbarButton onClick={() => applyFormat('insertUnorderedList')} tooltip="Bulleted List" disabled={isMarkdownMode}><BulletedListIcon className="w-5 h-5" /></ToolbarButton>
                            <ToolbarButton onClick={() => applyFormat('insertOrderedList')} tooltip="Numbered List" disabled={isMarkdownMode}><NumberedListIcon className="w-5 h-5" /></ToolbarButton>
                        </div>

                        <div className="w-px h-5 bg-gray-300 dark:bg-gray-600 mx-1 flex-shrink-0"></div>

                        {/* Insert Buttons */}
                        <div className="flex items-center flex-shrink-0">
                            <ToolbarButton onClick={applyLink} tooltip="Insert Link" disabled={isMarkdownMode}><LinkIcon className="w-5 h-5" /></ToolbarButton>
                            <ToolbarButton onClick={openImageModal} tooltip="Insert Image" disabled={isMarkdownMode}><ImageIcon className="w-5 h-5" /></ToolbarButton>
                            <ToolbarButton
                                buttonRef={tableBtnRef}
                                onClick={() => handleDropdownToggle('table', tableBtnRef)}
                                tooltip="Insert Table"
                                disabled={isMarkdownMode}
                            >
                                <TableIcon className="w-5 h-5" />
                            </ToolbarButton>
                            <ToolbarButton onClick={applyCodeBlock} tooltip="Code Block" disabled={isMarkdownMode}><CodeBlockIcon className="w-5 h-5" /></ToolbarButton>
                        </div>

                        {/* View Buttons */}
                        <div className="flex items-center flex-shrink-0">
                            <ToolbarButton onClick={toggleMarkdownMode} tooltip={isMarkdownMode ? "Rich Text View" : "Markdown Mode"}>
                                <MarkdownIcon className={`w-5 h-5 ${isMarkdownMode ? 'text-blue-600 dark:text-blue-400' : ''}`} />
                            </ToolbarButton>
                            <ToolbarButton onClick={toggleSourceMode} tooltip="Source Code"><SourceIcon className={`w-5 h-5 ${isSourceMode ? 'text-blue-600 dark:text-blue-400' : ''}`} /></ToolbarButton>
                            <ToolbarButton onClick={toggleFullScreen} tooltip={isFullScreen ? "Exit Fullscreen" : "Fullscreen"}>
                                {isFullScreen ? <ExitFullscreenIcon className="w-5 h-5" /> : <FullscreenIcon className="w-5 h-5" />}
                            </ToolbarButton>
                        </div>
                    </div>
                </div>
            </div>

            <div className="flex-grow relative overflow-hidden flex flex-col">
                <div className="flex-grow overflow-y-auto">
                    {isSourceMode ? (
                        <textarea
                            value={sourceContent}
                            onChange={handleSourceChange}
                            className="w-full h-full p-4 font-mono text-sm bg-gray-900 text-green-400 border-0 focus:outline-none resize-none"
                        />
                    ) : isMarkdownMode ? (
                        <textarea
                            value={markdownContent}
                            onChange={handleMarkdownChange}
                            className="w-full h-full p-4 font-mono text-sm bg-gray-50 dark:bg-gray-900 text-gray-800 dark:text-gray-200 border-0 focus:outline-none resize-none"
                        />
                    ) : (
                        <div
                            ref={editorRef}
                            contentEditable
                            onInput={handleInput}
                            onKeyDown={handleKeyDown}
                            className="prose dark:prose-invert max-w-none p-4 h-full focus:outline-none"
                        />
                    )}
                </div>

                {isCitationsOpen && citations.length > 0 && !isSourceMode && !isMarkdownMode && (
                    <div className="flex-none border-t border-gray-200 dark:border-gray-700 p-4 bg-gray-50 dark:bg-gray-900 max-h-60 overflow-y-auto shadow-inner transition-all duration-300">
                        <div className="flex items-center justify-between mb-3">
                            <h3 className="text-sm font-semibold text-gray-700 dark:text-gray-300 flex items-center">
                                <GoogleIcon className="w-4 h-4 mr-2" />
                                Sources & Citations
                            </h3>
                            <span className="text-xs text-gray-500 dark:text-gray-400">Powered by Google Search</span>
                        </div>
                        <ul className="grid grid-cols-1 md:grid-cols-2 gap-2">
                            {citations.map((citation, index) => (
                                <CitationItem key={index} citation={citation} index={index} />
                            ))}
                        </ul>
                    </div>
                )}
            </div>

            <div className="flex-none p-3 bg-gray-100 dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 flex justify-between items-center z-20">
                {/* Left: Citations Toggle */}
                <div>
                    {citations.length > 0 && (
                        <button
                            onClick={() => setIsCitationsOpen(!isCitationsOpen)}
                            className="flex items-center space-x-2 text-sm text-gray-700 dark:text-gray-300 hover:text-blue-600 dark:hover:text-blue-400 font-medium px-3 py-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 transition-colors"
                        >
                            <GoogleIcon className="w-4 h-4" />
                            <span>{citations.length} Citation{citations.length !== 1 ? 's' : ''}</span>
                            {isCitationsOpen ? <ChevronDownIcon className="w-4 h-4" /> : <ChevronUpIcon className="w-4 h-4" />}
                        </button>
                    )}
                </div>

                {/* Right: Magic Wand */}
                <button
                    onClick={openMagicWand}
                    className="flex items-center space-x-2 bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-md shadow-sm transition-colors text-sm font-medium"
                >
                    <MagicWandIcon className="w-5 h-5" />
                    <span>AI Edit</span>
                </button>
            </div>

            {showRestorePrompt && (
                <div className="absolute bottom-16 left-1/2 transform -translate-x-1/2 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 px-6 py-4 rounded-lg shadow-xl z-50 flex flex-col sm:flex-row items-center gap-4 animate-fade-in-up border border-gray-700 dark:border-gray-300">
                    <div className="flex flex-col">
                        <span className="font-semibold text-sm">Unsaved changes found</span>
                        <span className="text-xs text-gray-400 dark:text-gray-600">We found a draft from a previous session.</span>
                    </div>
                    <div className="flex gap-2 w-full sm:w-auto">
                        <button
                            onClick={handleRestoreDraft}
                            className="flex-1 sm:flex-none px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-xs font-bold rounded transition whitespace-nowrap"
                        >
                            Restore Draft
                        </button>
                        <button
                            onClick={handleDiscardDraft}
                            className="flex-1 sm:flex-none px-4 py-2 bg-transparent border border-gray-600 dark:border-gray-400 hover:bg-gray-800 dark:hover:bg-gray-200 text-white dark:text-gray-900 text-xs font-bold rounded transition whitespace-nowrap"
                        >
                            Discard
                        </button>
                    </div>
                </div>
            )}

            {isModalOpen && (
                <AiModal
                    onClose={() => { setIsModalOpen(false); setSavedRange(null); }}
                    aiPrompt={aiPrompt}
                    onAiPromptChange={(e) => setAiPrompt(e.target.value)}
                    handleAiRequest={() => handleAiRequest()}
                    isLoading={isLoading}
                    error={error}
                    selectedText={selectedText}
                    suggestions={selectedText ? suggestionsWithSelection : suggestionsWithoutSelection}
                    onSuggestionSelect={handleSuggestionSelect}
                    onQuickAction={handleQuickAction}
                />
            )}

            {isImageModalOpen && (
                <ImageInsertionModal
                    onClose={() => { setIsImageModalOpen(false); setSavedRange(null); }}
                    onInsert={handleImageInsert}
                />
            )}

            {/* Dropdown Panels */}
            {activeDropdown && dropdownPosition && (
                <div
                    ref={dropdownPanelRef}
                    style={{ position: 'fixed', top: `${dropdownPosition.top}px`, left: `${dropdownPosition.left}px` }}
                    className="z-50"
                >
                    {activeDropdown === 'paragraph' && (
                        <div onMouseLeave={() => setActiveSubMenu(null)} className="w-48 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg">
                            {mainCategories.map(category => (
                                <div key={category} onMouseEnter={() => setActiveSubMenu(category)} className="relative flex justify-between items-center px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer">
                                    <span>{category}</span>
                                    <ChevronRightIcon className="w-4 h-4" />
                                </div>
                            ))}
                            {activeSubMenu && menuStructure[activeSubMenu] && (
                                <div className="absolute top-0 left-full ml-1 w-56 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg">
                                    {menuStructure[activeSubMenu].map(item => (<SubMenuItem key={item.label} item={item} />))}
                                </div>
                            )}
                        </div>
                    )}
                    {activeDropdown === 'font' && (
                        <div className="w-48 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg">
                            {fontFamilies.map(font => (
                                <button
                                    key={font.name}
                                    onClick={() => applyFormat('fontName', font.value)}
                                    className="w-full text-left block px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
                                    style={{ fontFamily: font.value }}
                                >
                                    {font.name}
                                </button>
                            ))}
                        </div>
                    )}
                    {activeDropdown === 'fontSize' && (
                        <div className="w-32 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg">
                            {fontSizes.map(size => (
                                <button
                                    key={size}
                                    onClick={() => applyFontSize(size)}
                                    className="w-full text-left block px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
                                    style={{ fontSize: `${size}px` }}
                                >
                                    {size}px
                                </button>
                            ))}
                        </div>
                    )}
                    {activeDropdown === 'color' && (
                        <div className="w-44 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg p-2">
                            <div className="grid grid-cols-8 gap-1">
                                {colors.map(color => (
                                    <button
                                        key={color}
                                        onClick={() => applyFormat('foreColor', color)}
                                        className="w-5 h-5 rounded-sm border border-gray-300 dark:border-gray-600 hover:scale-125 transition-transform"
                                        style={{ backgroundColor: color }}
                                        aria-label={color}
                                    />
                                ))}
                            </div>
                        </div>
                    )}
                    {activeDropdown === 'table' && (
                        <div className="w-auto bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg p-2">
                            <div className="grid grid-cols-10 gap-1">
                                {Array.from({ length: 100 }).map((_, index) => {
                                    const row = Math.floor(index / 10) + 1;
                                    const col = (index % 10) + 1;
                                    const isHighlighted = row <= tableGridSize.rows && col <= tableGridSize.cols;
                                    return (
                                        <div
                                            key={index}
                                            onMouseEnter={() => setTableGridSize({ rows: row, cols: col })}
                                            onClick={() => insertTable(tableGridSize.rows, tableGridSize.cols)}
                                            className={`w-5 h-5 border border-gray-300 dark:border-gray-600 cursor-pointer transition-colors ${isHighlighted ? 'bg-blue-400 border-blue-500' : 'hover:bg-gray-200 dark:hover:bg-gray-600'
                                                }`}
                                        />
                                    );
                                })}
                            </div>
                            <div className="text-center text-sm mt-2 text-gray-600 dark:text-gray-400">
                                {tableGridSize.rows} x {tableGridSize.cols}
                            </div>
                        </div>
                    )}
                </div>
            )}


            <style>{`
        .prose :global(h1), .prose :global(h2), .prose :global(h3), .prose :global(h4), .prose :global(h5), .prose :global(h6) {
            margin-top: 1.25em;
            margin-bottom: 0.5em;
        }

        /* FIX: Enforce list styling for raw tags inside contentEditable */
        .prose ul {
            list-style-type: disc !important;
            padding-left: 1.625em !important;
            margin-top: 1em !important;
            margin-bottom: 1em !important;
        }
        .prose ol {
            list-style-type: decimal !important;
            padding-left: 1.625em !important;
            margin-top: 1em !important;
            margin-bottom: 1em !important;
        }
        .prose li {
            margin-top: 0.25em !important;
            margin-bottom: 0.25em !important;
            padding-left: 0.375em !important;
        }
        /* Ensure nested lists look distinct */
        .prose ul ul, .prose ol ul {
            list-style-type: circle !important;
        }
        .prose ol ol, .prose ul ol {
            list-style-type: lower-roman !important;
        }

        .animate-fade-in-up {
            animation: fadeInUp 0.3s ease-out forwards;
        }
        @keyframes fadeInUp {
            from {
                opacity: 0;
                transform: translateY(20px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
        .toolbar-scroll::-webkit-scrollbar {
            height: 6px;
        }
        .toolbar-scroll::-webkit-scrollbar-track {
            background: transparent;
        }
        .toolbar-scroll::-webkit-scrollbar-thumb {
            background-color: #cbd5e0; /* gray-400 */
            border-radius: 3px;
        }
        .toolbar-scroll:hover::-webkit-scrollbar-thumb {
             background-color: #a0aec0; /* gray-500 */
        }
        .dark .toolbar-scroll::-webkit-scrollbar-thumb {
            background-color: #4a5568; /* gray-600 */
        }
        .dark .toolbar-scroll:hover::-webkit-scrollbar-thumb {
            background-color: #718096; /* gray-500 */
        }
      `}</style>
        </div>
    );
};

export default GeminiEditor;