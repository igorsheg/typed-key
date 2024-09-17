/* eslint-disable node/prefer-global/process */
import * as fs from 'node:fs'
import * as os from 'node:os'
import * as path from 'node:path'
import type {
  ExtensionContext,
  OutputChannel,
  Uri,
  WorkspaceConfiguration,
} from 'vscode'
import {
  commands,
  window,
  workspace,
} from 'vscode'
import type {
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node'
import {
  DidChangeConfigurationNotification,
  Executable,
  LanguageClient,
} from 'vscode-languageclient/node'

let client: LanguageClient | undefined
let lastConfiguredPath: string | null = null

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
        const msg = err instanceof Error ? err.message : ''
        window.showErrorMessage(msg)
      }
    }),
  )

  context.subscriptions.push(
    workspace.onDidOpenTextDocument(async (document) => {
      if (client) {
        await updateConfiguration(client, document.uri)
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

  const env = { ...process.env, RUST_LOG: config.get('logLevel') }

  const serverOptions: ServerOptions = {
    run: { command: serverPath, options: { env } },
    debug: { command: serverPath, options: { env } },
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', pattern: '**' }, { scheme: 'untitled' }],
    initializationOptions: config,
    outputChannel,
    synchronize: { configurationSection: 'typedkey' },
  }

  const client = new LanguageClient('typedkey', 'TypedKey', serverOptions, clientOptions)

  const initialUri = workspace.workspaceFolders?.[0]?.uri
  if (initialUri) {
    await updateConfiguration(client, initialUri)
  }

  return client
}

async function updateConfiguration(client: LanguageClient, uri: Uri): Promise<void> {
  const packagePath = findPackagePath(uri.fsPath)
  if (packagePath) {
    const config = workspace.getConfiguration('typedkey', uri)
    const configPath = config.get<string>('translationsDir')
    if (configPath) {
      const fullResourcePath = path.join(packagePath, configPath)

      if (fullResourcePath !== lastConfiguredPath) {
        lastConfiguredPath = fullResourcePath
        const settings = { translationsDir: fullResourcePath }
        await client.sendNotification(DidChangeConfigurationNotification.type, { settings })
      }
    }
  }
}

function findPackagePath(filePath: string): string | null {
  let currentDir = path.dirname(filePath)

  while (true) {
    if (fs.existsSync(path.join(currentDir, 'package.json'))) {
      return currentDir
    }
    const parentDir = path.dirname(currentDir)
    if (currentDir === parentDir) {
      return null
    }
    currentDir = parentDir
  }
}

async function getServerPath(
  context: ExtensionContext,
  config: WorkspaceConfiguration,
): Promise<string> {
  let serverPath = process.env.TYPED_KEY_LSP_PATH ?? config.get<string>('path')

  if (serverPath) {
    if (serverPath.startsWith('~/')) {
      serverPath = path.join(os.homedir(), serverPath.slice(2))
    }
    if (fs.existsSync(serverPath)) {
      return serverPath
    }
    else {
      throw new Error(`${serverPath} does not exist. Please check 'typedkey.path' in Settings.`)
    }
  }

  const ext = process.platform === 'win32' ? '.exe' : ''
  const bundledPath = path.join(context.extensionPath, 'bundled', `typed-key${ext}`)

  if (fs.existsSync(bundledPath)) {
    return bundledPath
  }
  else {
    throw new Error(
      `TypedKey server binary not found. Please specify 'typedkey.path' in Settings or request a binary for your platform.`,
    )
  }
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop()
}
