/* eslint-disable ts/ban-ts-comment */
// @ts-nocheck
import React, { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

interface User {
  id: number
  name: string
  email: string
}

const StyledButton = styled.button`
  background-color: ${props => props.theme.primaryColor};
  color: white;
  padding: 10px 15px;
  border: none;
  border-radius: 4px;
`

const UserCard: React.FC<{ user: User }> = ({ user }) => {
  const { t } = useTranslation()
  return (
    <div>
      <h3>
        {t('user.name', { name: user.name })}
      </h3>
      <p>
        {' '}
        {t('user.email', { email: user.email })}
      </p>
    </div>
  )
}

const UserList: React.FC<{ users: User[] }> = ({ users }) => {
  const { t } = useTranslation()
  return (
    <div>
      <h2>
        {t('userList.title')}
        {' '}
      </h2>
      {users.map(user => <UserCard key={user.id} user={user} />)}
    </div>
  )
}

const ComplexComponent: React.FC = () => {
  const { t } = useTranslation()
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const fetchUsers = async () => {
      try {
        const response = await fetch('https://api.example.com/users')
        const data = await response.json()
        setUsers(data)
        setLoading(false)
      }
      catch (err) {
        setError(t('errors.fetchFailed'))
        setLoading(false)
      }
    }

    fetchUsers()
  }, [t])

  const handleRefresh = () => {
    setLoading(true)
    // Simulate refresh
    setTimeout(() => {
      setLoading(false)
      alert(t('alerts.refreshComplete'))
    }, 1000)
  }

  if (loading) {
    return (
      <div>
        {t('loading')}
        {' '}
      </div>
    )
  }

  if (error) {
    return (
      <div>
        <p>
          {t('errors.occurred')}
          {' '}
        </p>
        <p>
          {' '}
          {error}
          {' '}
        </p>
        <StyledButton onClick={handleRefresh}>
          {t('buttons.refresh')}
        </StyledButton>
      </div>
    )
  }

  return (
    <div>
      <h1>{t('app.title', { count: users.length })}</h1>
      <UserList users={users} />
      <StyledButton onClick={handleRefresh}>
        {t('buttons.refresh')}
      </StyledButton>
      <footer>
        <p>
          {
            t('app.footer', {
              year: new Date().getFullYear(),
              company: 'Acme Inc.',
            })
          }
        </p>
      </footer>
    </div>
  )
}

export default ComplexComponent
