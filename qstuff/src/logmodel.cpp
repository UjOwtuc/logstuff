#include "logmodel.h"

#include <QVariantList>
#include <QDateTime>
#include <QDebug>

LogModel::LogModel(const QStringList& columns, QObject* parent)
	: QAbstractTableModel(parent),
	m_columns(columns)
{}


int LogModel::columnCount(const QModelIndex&) const
{
	return m_columns.size();
}


int LogModel::rowCount(const QModelIndex&) const
{
	return m_data.size();
}


QVariant LogModel::data(const QModelIndex& index, int role) const
{
	if (index.isValid() && index.row() < m_data.size() && role == Qt::DisplayRole)
	{
		auto event = m_data[index.row()].toMap();
		return event["source"].toMap().value(m_columns[index.column()]);
	}
	return QVariant();
}


QVariant LogModel::headerData(int section, Qt::Orientation orientation, int role) const
{
	if (role == Qt::DisplayRole)
	{
		if (orientation == Qt::Horizontal)
		{
			if (section < m_columns.size())
				return m_columns[section];
			qDebug() << "requested invalid horizontal header" << section;
		}
		else
		{
			if (section < m_data.size())
				return m_data[section].toMap()["timestamp"].toDateTime().toString("yyyy-MM-dd hh:mm:ss.zzz");
			qDebug() << "requested invalid vertical header" << section;
		}
	}

	return QVariant();
}


void LogModel::setLogs(const QVariantList& data)
{
	if (!m_data.isEmpty())
	{
		beginRemoveRows(QModelIndex(), 0, m_data.size() -1);
		m_data.clear();
		endRemoveRows();
	}

	beginInsertRows(QModelIndex(), 0, data.size() -1);
	m_data = data;
	endInsertRows();
}


QVariant LogModel::rowData(int row) const
{
	if (row >= 0 && row < m_data.size())
		return m_data[row];
	return QVariant();
}


void LogModel::toggleColumn(const QString& name)
{
	if (m_columns.contains(name))
	{
		int index = m_columns.indexOf(name);
		beginRemoveColumns(QModelIndex(), index, index);
		m_columns.removeAt(index);
		endRemoveColumns();
	}
	else
	{
		beginInsertColumns(QModelIndex(), m_columns.size(), m_columns.size());
		m_columns.append(name);
		endInsertColumns();
	}
}


void LogModel::setColumns(const QStringList& columns)
{
	if (columns != m_columns)
	{
		if (m_columns.size())
		{
			beginRemoveColumns(QModelIndex(), 0, m_columns.size() -1);
			m_columns.clear();
			endRemoveColumns();
		}
		beginInsertColumns(QModelIndex(), 0, columns.size() -1);
		m_columns = columns;
		endInsertColumns();
	}
}
