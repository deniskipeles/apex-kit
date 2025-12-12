
export interface WebSource {
    uri: string;
    title: string;
}
  
export interface GroundingChunk {
    web: WebSource;
}

export interface GroundingMetadata {
    groundingChunks: GroundingChunk[];
}
