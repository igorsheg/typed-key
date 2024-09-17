/* eslint-disable node/prefer-global/process */
import * as fs from 'node:fs'
import * as os from 'node:os'
import * as path from 'node:path'
import type { ExtensionContext, OutputChannel } from 'vscode'
import { commands, window, workspace } from 'vscode'
import type {
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node'
import { LanguageClient } from 'vscode-languageclient/node'

let client: LanguageClient | undefined

export async function activate(context: ExtensionContext) {
  const outputChannel = window.createOutputChannel('TypedKey')
  context.subscriptions.push(outputChannel)

  context.subscriptions.push(
    workspace.onDidChangeConfiguration((e) => {
      if (
        ['typedkey.translationsDir', 'typedkey.logLevel', 'typedkey.path'].some(
          s => e.affectsConfiguration(s),
        )
      ) {
        commands.executeCommand('typedkey.restart')
      }
    }),
  )

  context.subscriptions.push(
    commands.registerCommand('typedkey.restart', async () => {
      if (client) {
        await client.stop()
      }

      try {
        client = await createClient(context, outputChannel)
        await client.start()
      }
      catch (err) {
        const msg
          = err instanceof Error ? err.message : 'An unknown error occurred'
        window.showErrorMessage(msg)
      }
    }),
  )

  await commands.executeCommand('typedkey.restart')
}

async function createClient(
  context: ExtensionContext,
  outputChannel: OutputChannel,
): Promise<LanguageClient> {
  const config = workspace.getConfiguration('typedkey')
  const serverPath = await getServerPath(context, config)

  outputChannel.appendLine(`Using TypedKey server at ${serverPath}`)

  const serverOptions: ServerOptions = {
    run: {
      command: serverPath,
      options: { env: { ...process.env, RUST_LOG: config.get('logLevel') } },
    },
    debug: {
      command: serverPath,
      options: { env: { ...process.env, RUST_LOG: config.get('logLevel') } },
    },
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'typescript' },
      { scheme: 'file', language: 'typescriptreact' },
      { scheme: 'file', language: 'javascript' },
      { scheme: 'file', language: 'javascriptreact' },
    ],
    synchronize: { configurationSection: 'typedkey' },
    initializationOptions: {
      translationsDir: config.get('translationsDir'),
      logLevel: config.get('logLevel'),
    },
    outputChannel,
  }

  return new LanguageClient(
    'typedkey',
    'TypedKey',
    serverOptions,
    clientOptions,
  )
}

async function getServerPath(
  context: ExtensionContext,
  config: any,
): Promise<string> {
  let serverPath = process.env.TYPED_KEY_LSP_PATH ?? config.get('path')

  if (serverPath) {
    if (serverPath.startsWith('~/')) {
      serverPath = path.join(os.homedir(), serverPath.slice(2))
    }
    if (fs.existsSync(serverPath)) {
      return serverPath
    }
    throw new Error(
      `${serverPath} does not exist. Please check 'typedkey.path' in Settings.`,
    )
  }

  const ext = process.platform === 'win32' ? '.exe' : ''
  const bundledPath = path.join(
    context.extensionPath,
    'bundled',
    `typed-key${ext}`,
  )

  if (fs.existsSync(bundledPath)) {
    return bundledPath
  }
  throw new Error(
    'TypedKey server binary not found. Please specify \'typedkey.path\' in Settings or request a binary for your platform.',
  )
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop()
}

