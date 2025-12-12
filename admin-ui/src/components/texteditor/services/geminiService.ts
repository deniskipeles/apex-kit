import { apiClient } from '../../../lib/apiClient'; // Ensure this path points to your sdk/client instance
import { GroundingMetadata } from '../types';

export interface GeminiResponse {
    text: string;
    metadata: GroundingMetadata | null;
}

/**
 * enhancing text using the server-side 'content-editor' action.
 */
export const enhanceTextWithGemini = async (
    originalText: string,
    prompt: string
): Promise<GeminiResponse> => {
    try {
        // We pass the variables defined in the Admin UI Template
        const response = await apiClient.ai.run('content-editor', {
            originalText: originalText,
            prompt: prompt
        });

        // The backend returns { result: string, ...extra_data }
        // Note: Ensure your backend passes through metadata if needed, 
        // otherwise this defaults to null.
        const text = response.result || "";
        const metadata = response?.metadata as GroundingMetadata || null;
        
        return { text, metadata };
    } catch (error) {
        console.error("Error calling AI Action 'content-editor':", error);
        throw new Error("Failed to process text via AI Action.");
    }
};

/**
 * Generates an image using the server-side 'generate-image' action.
 */
export const generateImageWithGemini = async (prompt: string): Promise<string> => {
    try {
        const response = await apiClient.ai.run('generate-image', {
            prompt: prompt
        });
        
        // Assuming the backend returns the base64 string in the 'result' field
        // for image generation models.
        if (response.result) {
             // Ensure it has the data URI prefix if the backend sends raw base64
             if (!response.result.startsWith('data:')) {
                 // Defaulting to png, adjust based on model output
                 return `data:image/png;base64,${response.result}`;
             }
             return response.result;
        }
        
        throw new Error("No image content generated");
    } catch (error) {
        console.error("Error calling AI Action 'generate-image':", error);
        throw new Error("Failed to generate image.");
    }
};

/**
 * Edits an image using the server-side 'edit-image' action.
 */
export const editImageWithGemini = async (base64Image: string, prompt: string): Promise<string> => {
    try {
        // We send the base64 image as a variable. 
        // The backend script/handler must know how to handle a variable named 'image'.
        const response = await apiClient.ai.run('edit-image', {
            image: base64Image, 
            prompt: prompt
        });

        if (response.result) {
             if (!response.result.startsWith('data:')) {
                 return `data:image/png;base64,${response.result}`;
             }
             return response.result;
        }
        
        throw new Error("No edited image content returned");
    } catch (error) {
        console.error("Error calling AI Action 'edit-image':", error);
        throw new Error("Failed to edit image.");
    }
};