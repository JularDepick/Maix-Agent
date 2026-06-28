export class MaixError extends Error {
  constructor(
    message: string,
    public code: string,
    public cause?: Error
  ) {
    super(message);
    this.name = 'MaixError';
  }
}

export class ProviderError extends MaixError {
  constructor(message: string, cause?: Error) {
    super(message, 'PROVIDER_ERROR', cause);
    this.name = 'ProviderError';
  }
}

export class ToolError extends MaixError {
  constructor(message: string, cause?: Error) {
    super(message, 'TOOL_ERROR', cause);
    this.name = 'ToolError';
  }
}

export class ConfigError extends MaixError {
  constructor(message: string, cause?: Error) {
    super(message, 'CONFIG_ERROR', cause);
    this.name = 'ConfigError';
  }
}

export class SessionError extends MaixError {
  constructor(message: string, cause?: Error) {
    super(message, 'SESSION_ERROR', cause);
    this.name = 'SessionError';
  }
}
