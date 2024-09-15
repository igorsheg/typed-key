/* eslint-disable node/prefer-global/process */
import * as os from 'node:os'
import * as path from 'node:path'
import * as fs from 'node:fs'
import type { ConfigurationChangeEvent, ExtensionContext, OutputChannel, WorkspaceConfiguration } from 'vscode'
import { Uri, commands, window, workspace } from 'vscode'

import type {
  Executable,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node'
import {
  DidChangeConfigurationNotification,
  LanguageClient,
} from 'vscode-languageclient/node'

let client: LanguageClient | undefined
let lastConfiguredPath: string | null = null

export async function activate(
  context: ExtensionContext,
): Promise<void> {
  const name = 'TypedKey'

  const outputChannel = window.createOutputChannel(name)

  context.subscriptions.push(outputChannel)

  context.subscriptions.push(
    workspace.onDidChangeConfiguration(
      async (e: ConfigurationChangeEvent) => {
        const restartTriggeredBy = [
          'typedkey.translationsDir',
          'typedkey.logLevel',
          'typedkey.path',
        ].find(s => e.affectsConfiguration(s))

        if (restartTriggeredBy) {
          await commands.executeCommand('typedkey.restart')
        }
      },
    ),
  )

  context.subscriptions.push(
    commands.registerCommand('typedkey.restart', async () => {
      if (client && client.needsStop()) {
        await client.stop()
      }

      try {
        client = await createClient(context, name, outputChannel)
      }
      catch (err) {
        const msg = err instanceof Error ? err.message : ''
        window.showErrorMessage(`${msg}`)
        return
      }

      await client.start()
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
  name: string,
  outputChannel: OutputChannel,
): Promise<LanguageClient> {
  const env = { ...process.env }

  const config = workspace.getConfiguration('typedkey')
  const serverPath = await getServerPath(context, config)

  outputChannel.appendLine(`Using typedkey server ${serverPath}`)

  env.RUST_LOG = config.get('logLevel')

  const run: Executable = {
    command: serverPath,
    options: { env },
  }

  const serverOptions: ServerOptions = {
    run,
    debug: run,
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'untitled' },
      { scheme: 'file', pattern: '**' },
      { scheme: 'vscode-scm' },
    ],
    initializationOptions: config,
    outputChannel,
    traceOutputChannel: outputChannel,
    synchronize: {
      configurationSection: 'typedkey',
    },
  }

  const client = new LanguageClient(
    name.toLowerCase(),
    name,
    serverOptions,
    clientOptions,
  )

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

      // Only send a notification if the path has changed
      if (fullResourcePath !== lastConfiguredPath) {
        lastConfiguredPath = fullResourcePath

        const settings = {
          translationsDir: fullResourcePath,
        }

        await client.sendNotification(DidChangeConfigurationNotification.type, {
          settings,
        })
      }
    }
  }
}

function findPackagePath(filePath: string): string | null {
  let currentDir = path.dirname(filePath)

  while (currentDir !== path.dirname(currentDir)) { // Stop at root
    if (fs.existsSync(path.join(currentDir, 'package.json'))) {
      return currentDir
    }
    currentDir = path.dirname(currentDir)
  }

  return null
}

async function getServerPath(
  context: ExtensionContext,
  config: WorkspaceConfiguration,
): Promise<string> {
  let path
    = process.env.TYPED_KEY_LSP_PATH ?? config.get<null | string>('path')

  if (path) {
    if (path.startsWith('~/')) {
      path = os.homedir() + path.slice('~'.length)
    }
    const pathUri = Uri.file(path)

    return await workspace.fs.stat(pathUri).then(
      () => pathUri.fsPath,
      () => {
        throw new Error(
          `${path} does not exist. Please check typedkey.path in Settings.`,
        )
      },
    )
  }

  const ext = process.platform === 'win32' ? '.exe' : ''
  const bundled = Uri.joinPath(
    context.extensionUri,
    'bundled',
    `typed-key${ext}`,
  )

  return await workspace.fs.stat(bundled).then(
    () => bundled.fsPath,
    () => {
      throw new Error(
        `Unfortunately we don't ship binaries for your platform yet. ${bundled.toString()} `
        + 'Try specifying typedkey.path in Settings. '
        + 'Or raise an issue [here](https://github.com/igorsheg/typed-key/issues) '
        + 'to request a binary for your platform.',
      )
    },
  )
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined
  }
  return client.stop()
}
